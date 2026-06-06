//! Pagination — offset, cursor, and keyset strategies.
//!
//! Three complementary strategies are provided:
//!
//! - **Offset** (`OffsetPageRequest` / `Page<T>`) — classic `LIMIT … OFFSET …`,
//!   suitable for UI pages with a known total count.
//! - **Cursor** (`CursorPageRequest` / `CursorPage<T>`) — opaque, base64-encoded
//!   position tokens; ideal for feeds and activity streams.
//! - **Keyset** (`KeysetPageRequest` / `KeysetPage<T>`) — compound column comparison
//!   (`WHERE (col1, col2) > (v1, v2)`); most efficient for large sorted tables.
//!
//! Optional SQL helpers live in the `sql` sub-module (feature = `"sql"`).

#[cfg(feature = "sql")]
pub mod sql;

use std::fmt;
use std::str::FromStr;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::error::DataError;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Default number of items per page when no explicit `per_page` is supplied.
pub const DEFAULT_PAGE_SIZE: u32 = 20;

/// Hard upper limit on items per page — requests for more are silently capped.
pub const MAX_PAGE_SIZE: u32 = 100;

// ── Shared types ──────────────────────────────────────────────────────────────

/// Sort direction for a single sort key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    /// Ascending order (smallest first). This is the default.
    #[default]
    Asc,
    /// Descending order (largest first).
    Desc,
}

/// One sort criterion — a field name together with a direction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortKey {
    /// The field (column) name to sort on.
    pub field: String,
    /// The direction of the sort.
    pub order: SortOrder,
}

impl SortKey {
    /// Create an ascending sort key for `field`.
    pub fn asc(field: impl Into<String>) -> Self {
        SortKey { field: field.into(), order: SortOrder::Asc }
    }

    /// Create a descending sort key for `field`.
    pub fn desc(field: impl Into<String>) -> Self {
        SortKey { field: field.into(), order: SortOrder::Desc }
    }
}

/// SQL parameter placeholder dialect for keyset and offset pagination helpers.
///
/// Pass this to [`sql::keyset_where_clause`]
/// to select between positional (`$1`, `$2`, …) and question-mark (`?`, `?`, …)
/// parameter styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeysetDialect {
    /// PostgreSQL-style positional parameters: `$1`, `$2`, …
    Positional,
    /// MySQL / SQLite-style question-mark parameters: `?`, `?`, …
    Question,
}

// ── 1. Offset-based pagination ─────────────────────────────────────────────────

/// A request for a single page of results using classic `LIMIT … OFFSET …`.
///
/// Pages are **1-indexed** (`page = 1` is the first page). `per_page` is
/// silently capped at [`MAX_PAGE_SIZE`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OffsetPageRequest {
    /// Current page number (1-indexed, must be ≥ 1).
    pub page: u32,
    /// Maximum number of items per page (capped at [`MAX_PAGE_SIZE`]).
    pub per_page: u32,
    /// Optional sort keys; an empty slice means caller-supplied default ordering.
    pub sort: Vec<SortKey>,
}

impl OffsetPageRequest {
    /// Create a new request, validating that `page >= 1` and capping
    /// `per_page` at [`MAX_PAGE_SIZE`].
    pub fn new(page: u32, per_page: u32) -> Result<Self, DataError> {
        if page < 1 {
            return Err(DataError::InvalidPage(
                "page must be >= 1 (pages are 1-indexed)".into(),
            ));
        }
        let per_page = per_page.clamp(1, MAX_PAGE_SIZE);
        Ok(OffsetPageRequest { page, per_page, sort: Vec::new() })
    }

    /// Convenience: create a request for the **first** page with `per_page` items.
    pub fn first(per_page: u32) -> Result<Self, DataError> {
        Self::new(1, per_page)
    }

    /// The SQL `OFFSET` value: `(page - 1) * per_page`.
    pub fn offset(&self) -> u64 {
        (self.page as u64 - 1) * self.per_page as u64
    }

    /// The SQL `LIMIT` value: equal to `per_page`.
    pub fn limit(&self) -> u64 {
        self.per_page as u64
    }
}

impl Default for OffsetPageRequest {
    fn default() -> Self {
        OffsetPageRequest {
            page: 1,
            per_page: DEFAULT_PAGE_SIZE,
            sort: Vec::new(),
        }
    }
}

/// A single page of results with full metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page<T> {
    /// The items on this page.
    pub items: Vec<T>,
    /// Total number of matching items across all pages.
    pub total_items: u64,
    /// The current page number (1-indexed).
    pub page: u32,
    /// The maximum items per page that was requested.
    pub per_page: u32,
    /// Total number of pages (`ceil(total_items / per_page)`).
    pub total_pages: u64,
    /// Whether there is a next page.
    pub has_next: bool,
    /// Whether there is a previous page.
    pub has_prev: bool,
}

impl<T> Page<T> {
    /// Build a `Page` from `items`, the overall `total_items` count, and the
    /// original `request`. All derived fields are computed automatically.
    pub fn new(items: Vec<T>, total_items: u64, req: &OffsetPageRequest) -> Self {
        let per_page = req.per_page as u64;
        let total_pages = if per_page == 0 {
            0
        } else {
            total_items.div_ceil(per_page)
        };
        let page = req.page;
        let has_prev = page > 1;
        let has_next = (page as u64) < total_pages;
        Page { items, total_items, page, per_page: req.per_page, total_pages, has_next, has_prev }
    }

    /// Build an empty `Page` (zero items, zero total).
    pub fn empty(req: &OffsetPageRequest) -> Self {
        Page::new(Vec::new(), 0, req)
    }

    /// Transform every item with `f`, preserving all pagination metadata.
    pub fn map<U, F: FnMut(T) -> U>(self, f: F) -> Page<U> {
        Page {
            items: self.items.into_iter().map(f).collect(),
            total_items: self.total_items,
            page: self.page,
            per_page: self.per_page,
            total_pages: self.total_pages,
            has_next: self.has_next,
            has_prev: self.has_prev,
        }
    }
}

// ── 2. Cursor-based pagination ─────────────────────────────────────────────────

/// An opaque, URL-safe, base64-encoded position token used by cursor pagination.
///
/// The inner string is `URL_SAFE_NO_PAD`-encoded JSON; callers treat it as
/// opaque.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Cursor(String);

impl Cursor {
    /// Serialize `value` to JSON and base64-url-safe-encode it (no padding).
    pub fn encode<T: Serialize>(value: &T) -> Result<Cursor, DataError> {
        let json = serde_json::to_string(value)
            .map_err(|e| DataError::InvalidCursor(e.to_string()))?;
        Ok(Cursor(URL_SAFE_NO_PAD.encode(json.as_bytes())))
    }

    /// Decode and deserialize a cursor back to `T`.
    pub fn decode<T: DeserializeOwned>(&self) -> Result<T, DataError> {
        let bytes = URL_SAFE_NO_PAD
            .decode(self.0.as_bytes())
            .map_err(|e| DataError::InvalidCursor(format!("base64 decode: {e}")))?;
        serde_json::from_slice(&bytes)
            .map_err(|e| DataError::InvalidCursor(format!("json decode: {e}")))
    }

    /// Return the raw base64 string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Cursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Cursor {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Cursor(s.to_owned()))
    }
}

/// A request for one page of results using opaque cursor tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPageRequest {
    /// Forward: fetch items after this cursor. `None` = start from beginning.
    pub after: Option<Cursor>,
    /// Backward: fetch items before this cursor. `None` = no upper bound.
    pub before: Option<Cursor>,
    /// Maximum number of items to return (capped at [`MAX_PAGE_SIZE`], ≥ 1).
    pub limit: u32,
    /// Sort keys; ordering must be stable for cursors to be meaningful.
    pub sort: Vec<SortKey>,
}

impl CursorPageRequest {
    /// Create a new request with `limit` items per page. `limit` is validated
    /// (≥ 1) and capped at [`MAX_PAGE_SIZE`].
    pub fn new(limit: u32) -> Result<Self, DataError> {
        if limit < 1 {
            return Err(DataError::InvalidPage("cursor limit must be >= 1".into()));
        }
        let limit = limit.min(MAX_PAGE_SIZE);
        Ok(CursorPageRequest { after: None, before: None, limit, sort: Vec::new() })
    }

    /// Set the `after` cursor (forward pagination).
    pub fn after(mut self, cursor: Cursor) -> Self {
        self.after = Some(cursor);
        self
    }

    /// Set the `before` cursor (backward pagination).
    pub fn before(mut self, cursor: Cursor) -> Self {
        self.before = Some(cursor);
        self
    }

    /// Set the sort keys.
    pub fn sort(mut self, keys: Vec<SortKey>) -> Self {
        self.sort = keys;
        self
    }
}

impl Default for CursorPageRequest {
    fn default() -> Self {
        CursorPageRequest {
            after: None,
            before: None,
            limit: DEFAULT_PAGE_SIZE,
            sort: Vec::new(),
        }
    }
}

/// A page of results from cursor-based pagination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPage<T> {
    /// The items on this page.
    pub items: Vec<T>,
    /// Cursor pointing at the first item (for backward paging).
    pub start_cursor: Option<Cursor>,
    /// Cursor pointing at the last item (for forward paging).
    pub end_cursor: Option<Cursor>,
    /// Whether there are more items after this page.
    pub has_next_page: bool,
    /// Whether there are items before this page.
    pub has_prev_page: bool,
}

impl<T> CursorPage<T> {
    /// Build a `CursorPage` from `items`.
    ///
    /// `encode` extracts the cursor value `C` from each item (applied to the
    /// first and last items to build `start_cursor` / `end_cursor`).
    pub fn from_items<C, F>(
        items: Vec<T>,
        encode: F,
        has_prev: bool,
        has_next: bool,
    ) -> Result<Self, DataError>
    where
        F: Fn(&T) -> C,
        C: Serialize,
    {
        let start_cursor = items
            .first()
            .map(|item| Cursor::encode(&encode(item)))
            .transpose()?;
        let end_cursor = items
            .last()
            .map(|item| Cursor::encode(&encode(item)))
            .transpose()?;
        Ok(CursorPage {
            items,
            start_cursor,
            end_cursor,
            has_next_page: has_next,
            has_prev_page: has_prev,
        })
    }

    /// Transform every item with `f`, preserving cursor metadata.
    pub fn map<U, F: FnMut(T) -> U>(self, f: F) -> CursorPage<U> {
        CursorPage {
            items: self.items.into_iter().map(f).collect(),
            start_cursor: self.start_cursor,
            end_cursor: self.end_cursor,
            has_next_page: self.has_next_page,
            has_prev_page: self.has_prev_page,
        }
    }
}

// ── 3. Keyset-based pagination ─────────────────────────────────────────────────

/// The sort-column values at a page boundary, used in the keyset WHERE clause.
///
/// Values are ordered to match the `sort` keys in the request. Encoded and
/// decoded via [`Cursor::encode`] / [`Cursor::decode`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeysetPosition {
    /// Ordered sort-column values matching the request's `sort` keys.
    pub values: Vec<serde_json::Value>,
}

/// A request for one page of results using keyset (seek-method) pagination.
///
/// Keyset pagination queries `WHERE (col1, col2, …) > (v1, v2, …)` which is
/// index-efficient and stable under concurrent inserts/deletes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeysetPageRequest {
    /// The sort columns that form the keyset. Must have ≥ 1 key.
    pub sort: Vec<SortKey>,
    /// Cursor encoding a [`KeysetPosition`]. `None` = fetch from the start.
    pub after: Option<Cursor>,
    /// Maximum number of items to return (capped at [`MAX_PAGE_SIZE`], ≥ 1).
    pub limit: u32,
}

impl KeysetPageRequest {
    /// Create a new keyset request. Validates that `sort` has ≥ 1 key and
    /// that `limit` is ≥ 1 (and caps it at [`MAX_PAGE_SIZE`]).
    pub fn new(sort: Vec<SortKey>, limit: u32) -> Result<Self, DataError> {
        if sort.is_empty() {
            return Err(DataError::InvalidPage(
                "keyset pagination requires at least one sort key".into(),
            ));
        }
        if limit < 1 {
            return Err(DataError::InvalidPage("keyset limit must be >= 1".into()));
        }
        let limit = limit.min(MAX_PAGE_SIZE);
        Ok(KeysetPageRequest { sort, after: None, limit })
    }

    /// Decode the `after` cursor to a [`KeysetPosition`], if present.
    pub fn decoded_after(&self) -> Result<Option<KeysetPosition>, DataError> {
        self.after
            .as_ref()
            .map(|c| c.decode::<KeysetPosition>())
            .transpose()
    }
}

/// A page of results from keyset-based pagination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeysetPage<T> {
    /// The items on this page.
    pub items: Vec<T>,
    /// Cursor pointing at the first item.
    pub start_cursor: Option<Cursor>,
    /// Cursor pointing at the last item.
    pub end_cursor: Option<Cursor>,
    /// Whether there are more items after this page.
    pub has_next_page: bool,
    /// Whether there are items before this page.
    pub has_prev_page: bool,
}

impl<T> KeysetPage<T> {
    /// Build a `KeysetPage` from `items`.
    ///
    /// `encode` extracts the keyset column values (as `Vec<serde_json::Value>`)
    /// from each item; the first and last items become cursors encoding a
    /// [`KeysetPosition`].
    pub fn from_items<F>(
        items: Vec<T>,
        encode: F,
        has_prev: bool,
        has_next: bool,
    ) -> Result<Self, DataError>
    where
        F: Fn(&T) -> Vec<serde_json::Value>,
    {
        let start_cursor = items
            .first()
            .map(|item| Cursor::encode(&KeysetPosition { values: encode(item) }))
            .transpose()?;
        let end_cursor = items
            .last()
            .map(|item| Cursor::encode(&KeysetPosition { values: encode(item) }))
            .transpose()?;
        Ok(KeysetPage {
            items,
            start_cursor,
            end_cursor,
            has_next_page: has_next,
            has_prev_page: has_prev,
        })
    }

    /// Transform every item with `f`, preserving cursor metadata.
    pub fn map<U, F: FnMut(T) -> U>(self, f: F) -> KeysetPage<U> {
        KeysetPage {
            items: self.items.into_iter().map(f).collect(),
            start_cursor: self.start_cursor,
            end_cursor: self.end_cursor,
            has_next_page: self.has_next_page,
            has_prev_page: self.has_prev_page,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── OffsetPageRequest ──────────────────────────────────────────────────────

    #[test]
    fn offset_request_default() {
        let req = OffsetPageRequest::default();
        assert_eq!(req.page, 1);
        assert_eq!(req.per_page, DEFAULT_PAGE_SIZE);
        assert!(req.sort.is_empty());
    }

    #[test]
    fn offset_request_new_valid() {
        let req = OffsetPageRequest::new(3, 10).unwrap();
        assert_eq!(req.page, 3);
        assert_eq!(req.per_page, 10);
        assert_eq!(req.offset(), 20);
        assert_eq!(req.limit(), 10);
    }

    #[test]
    fn offset_request_first() {
        let req = OffsetPageRequest::first(5).unwrap();
        assert_eq!(req.page, 1);
        assert_eq!(req.offset(), 0);
        assert_eq!(req.limit(), 5);
    }

    #[test]
    fn offset_request_page_zero_is_error() {
        let err = OffsetPageRequest::new(0, 10).unwrap_err();
        match err {
            DataError::InvalidPage(msg) => assert!(msg.contains("1-indexed")),
            other => panic!("expected InvalidPage, got {other:?}"),
        }
    }

    #[test]
    fn offset_request_per_page_capped() {
        let req = OffsetPageRequest::new(1, 9999).unwrap();
        assert_eq!(req.per_page, MAX_PAGE_SIZE);
    }

    // ── Page<T> ───────────────────────────────────────────────────────────────

    #[test]
    fn page_new_computes_metadata() {
        let req = OffsetPageRequest::new(2, 10).unwrap();
        let page: Page<i32> = Page::new(vec![1, 2, 3], 25, &req);
        assert_eq!(page.total_pages, 3); // ceil(25/10)
        assert!(page.has_prev); // page 2 has prev
        assert!(page.has_next); // page 2 of 3 has next
    }

    #[test]
    fn page_last_page_has_no_next() {
        let req = OffsetPageRequest::new(3, 10).unwrap();
        let page: Page<i32> = Page::new(vec![1], 21, &req);
        assert_eq!(page.total_pages, 3);
        assert!(page.has_prev);
        assert!(!page.has_next);
    }

    #[test]
    fn page_first_page_has_no_prev() {
        let req = OffsetPageRequest::new(1, 10).unwrap();
        let page: Page<i32> = Page::new(vec![1], 5, &req);
        assert!(!page.has_prev);
    }

    #[test]
    fn page_empty() {
        let req = OffsetPageRequest::default();
        let page: Page<i32> = Page::empty(&req);
        assert!(page.items.is_empty());
        assert_eq!(page.total_items, 0);
        assert_eq!(page.total_pages, 0);
        assert!(!page.has_prev);
        assert!(!page.has_next);
    }

    #[test]
    fn page_map_transforms_items_preserves_meta() {
        let req = OffsetPageRequest::new(2, 5).unwrap();
        let page = Page::new(vec![1u32, 2, 3], 13, &req);
        let mapped = page.map(|x| x * 10);
        assert_eq!(mapped.items, vec![10, 20, 30]);
        assert_eq!(mapped.total_items, 13);
        assert_eq!(mapped.total_pages, 3);
        assert!(mapped.has_prev);
        assert!(mapped.has_next);
    }

    // ── Cursor ────────────────────────────────────────────────────────────────

    #[test]
    fn cursor_encode_decode_round_trip() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Pos {
            id: u64,
            ts: i64,
        }

        let pos = Pos { id: 42, ts: 1_700_000_000 };
        let cursor = Cursor::encode(&pos).unwrap();
        let decoded: Pos = cursor.decode().unwrap();
        assert_eq!(decoded, pos);
    }

    #[test]
    fn cursor_as_str_and_display() {
        let cursor = Cursor::encode(&42u32).unwrap();
        assert_eq!(cursor.as_str(), cursor.to_string());
    }

    #[test]
    fn cursor_from_str() {
        let c: Cursor = "abc123".parse().unwrap();
        assert_eq!(c.as_str(), "abc123");
    }

    #[test]
    fn cursor_decode_garbage_returns_invalid_cursor() {
        let bad: Cursor = "!!!!not-valid-base64!!!!".parse().unwrap();
        let err = bad.decode::<u32>().unwrap_err();
        match err {
            DataError::InvalidCursor(_) => {}
            other => panic!("expected InvalidCursor, got {other:?}"),
        }
    }

    #[test]
    fn cursor_decode_valid_base64_but_bad_json_returns_invalid_cursor() {
        let c: Cursor = URL_SAFE_NO_PAD.encode(b"not-json").parse().unwrap();
        let err = c.decode::<u32>().unwrap_err();
        match err {
            DataError::InvalidCursor(_) => {}
            other => panic!("expected InvalidCursor, got {other:?}"),
        }
    }

    // ── CursorPage<T> ─────────────────────────────────────────────────────────

    #[test]
    fn cursor_page_from_items_sets_cursors_and_flags() {
        let items = vec![10u32, 20, 30];
        let page = CursorPage::from_items(items, |x| *x, false, true).unwrap();
        assert_eq!(page.items, vec![10, 20, 30]);
        assert!(page.start_cursor.is_some());
        assert!(page.end_cursor.is_some());
        assert!(page.has_next_page);
        assert!(!page.has_prev_page);

        let start: u32 = page.start_cursor.unwrap().decode().unwrap();
        let end: u32 = page.end_cursor.unwrap().decode().unwrap();
        assert_eq!(start, 10);
        assert_eq!(end, 30);
    }

    #[test]
    fn cursor_page_empty_has_no_cursors() {
        let page: CursorPage<u32> =
            CursorPage::from_items(vec![], |x| *x, false, false).unwrap();
        assert!(page.start_cursor.is_none());
        assert!(page.end_cursor.is_none());
    }

    #[test]
    fn cursor_page_map() {
        let items = vec![1u32, 2, 3];
        let page = CursorPage::from_items(items, |x| *x, true, false).unwrap();
        let mapped = page.map(|x| x.to_string());
        assert_eq!(mapped.items, vec!["1", "2", "3"]);
        assert!(mapped.has_prev_page);
        assert!(!mapped.has_next_page);
    }

    // ── KeysetPage<T> ─────────────────────────────────────────────────────────

    #[test]
    fn keyset_page_builds_correctly() {
        let items = vec![("alice", 100i64), ("bob", 200)];
        let page = KeysetPage::from_items(
            items,
            |(name, ts)| {
                vec![
                    serde_json::Value::String((*name).to_string()),
                    serde_json::Value::Number(serde_json::Number::from(*ts)),
                ]
            },
            false,
            true,
        )
        .unwrap();
        assert_eq!(page.items.len(), 2);
        assert!(page.start_cursor.is_some());
        assert!(page.end_cursor.is_some());
        assert!(page.has_next_page);

        let pos: KeysetPosition = page.start_cursor.unwrap().decode().unwrap();
        assert_eq!(pos.values.len(), 2);
        assert_eq!(pos.values[0].as_str().unwrap(), "alice");
    }

    #[test]
    fn keyset_request_validates_sort_nonempty() {
        let err = KeysetPageRequest::new(vec![], 10).unwrap_err();
        match err {
            DataError::InvalidPage(msg) => assert!(msg.contains("sort key")),
            other => panic!("expected InvalidPage, got {other:?}"),
        }
    }

    #[test]
    fn keyset_request_limit_capped() {
        let req = KeysetPageRequest::new(vec![SortKey::asc("id")], 9999).unwrap();
        assert_eq!(req.limit, MAX_PAGE_SIZE);
    }
}
