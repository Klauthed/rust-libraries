//! Keyset (seek-method) pagination: [`KeysetPosition`], [`KeysetPageRequest`],
//! and [`KeysetPage`].

use serde::{Deserialize, Serialize};

use crate::error::DataError;

use super::{Cursor, MAX_PAGE_SIZE, SortKey};

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
        self.after.as_ref().map(|c| c.decode::<KeysetPosition>()).transpose()
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

#[cfg(test)]
mod tests {
    use super::*;

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
