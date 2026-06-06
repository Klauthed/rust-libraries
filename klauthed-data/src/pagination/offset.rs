//! Offset-based pagination: [`OffsetPageRequest`] and [`Page`].

use serde::{Deserialize, Serialize};

use crate::error::DataError;

use super::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE, SortKey};

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
            return Err(DataError::InvalidPage("page must be >= 1 (pages are 1-indexed)".into()));
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
        OffsetPageRequest { page: 1, per_page: DEFAULT_PAGE_SIZE, sort: Vec::new() }
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
        let total_pages = if per_page == 0 { 0 } else { total_items.div_ceil(per_page) };
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
