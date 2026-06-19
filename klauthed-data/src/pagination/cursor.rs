//! Cursor-based pagination: opaque [`Cursor`] tokens, [`CursorPageRequest`],
//! and [`CursorPage`].

use std::fmt;
use std::str::FromStr;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::error::DataError;

use super::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE, SortKey};

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
        let json =
            serde_json::to_string(value).map_err(|e| DataError::InvalidCursor(e.to_string()))?;
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
    #[must_use]
    pub fn after(mut self, cursor: Cursor) -> Self {
        self.after = Some(cursor);
        self
    }

    /// Set the `before` cursor (backward pagination).
    #[must_use]
    pub fn before(mut self, cursor: Cursor) -> Self {
        self.before = Some(cursor);
        self
    }

    /// Set the sort keys.
    #[must_use]
    pub fn sort(mut self, keys: Vec<SortKey>) -> Self {
        self.sort = keys;
        self
    }
}

impl Default for CursorPageRequest {
    fn default() -> Self {
        CursorPageRequest { after: None, before: None, limit: DEFAULT_PAGE_SIZE, sort: Vec::new() }
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
        let start_cursor = items.first().map(|item| Cursor::encode(&encode(item))).transpose()?;
        let end_cursor = items.last().map(|item| Cursor::encode(&encode(item))).transpose()?;
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let page: CursorPage<u32> = CursorPage::from_items(vec![], |x| *x, false, false).unwrap();
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
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Any serializable value survives an `encode` → `decode` round-trip.
        #[test]
        fn encode_decode_round_trips(id in any::<u64>(), ts in any::<i64>(), name in "[ -~]{0,32}") {
            let value = (id, ts, name);
            let cursor = Cursor::encode(&value).unwrap();
            let decoded: (u64, i64, String) = cursor.decode().unwrap();
            prop_assert_eq!(decoded, value);
        }

        /// `as_str`, `Display`, and `FromStr` agree on the opaque token text.
        #[test]
        fn string_forms_agree(id in any::<u64>()) {
            let cursor = Cursor::encode(&id).unwrap();
            let shown = cursor.to_string();
            prop_assert_eq!(cursor.as_str(), shown.as_str());
            let reparsed: Cursor = cursor.as_str().parse().unwrap(); // FromStr is infallible
            prop_assert_eq!(reparsed.as_str(), cursor.as_str());
        }

        /// Decoding arbitrary text errors gracefully rather than panicking.
        #[test]
        fn decode_arbitrary_text_never_panics(s in ".*") {
            let cursor: Cursor = s.parse().unwrap(); // FromStr is infallible
            let _ = cursor.decode::<(u64, i64, String)>();
        }
    }
}
