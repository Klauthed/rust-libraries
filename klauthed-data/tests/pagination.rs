//! Public-API integration test for offset pagination (no backend required).

use klauthed_data::pagination::{OffsetPageRequest, Page};

#[test]
fn offset_request_and_page_metadata() {
    let req = OffsetPageRequest::new(2, 10).unwrap();
    assert_eq!(req.offset(), 10); // (page 2 - 1) * 10
    assert_eq!(req.limit(), 10);

    let page: Page<i32> = Page::new(vec![11, 12, 13], 25, &req);
    assert_eq!(page.total_pages, 3); // ceil(25 / 10)
    assert!(page.has_prev);
    assert!(page.has_next);

    // `map` preserves pagination metadata.
    let mapped = page.map(|n| n.to_string());
    assert_eq!(mapped.items, ["11", "12", "13"]);
    assert_eq!(mapped.total_pages, 3);
}

#[test]
fn page_one_has_no_previous() {
    let req = OffsetPageRequest::first(20).unwrap();
    let page: Page<i32> = Page::empty(&req);
    assert!(!page.has_prev);
    assert_eq!(page.total_items, 0);
}
