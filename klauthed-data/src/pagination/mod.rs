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

pub mod cursor;
pub mod keyset;
pub mod offset;

pub use cursor::{Cursor, CursorPage, CursorPageRequest};
pub use keyset::{KeysetPage, KeysetPageRequest, KeysetPosition};
pub use offset::{OffsetPageRequest, Page};

use serde::{Deserialize, Serialize};

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
