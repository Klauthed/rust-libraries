//! SQL helper utilities for building paginated queries.
//!
//! These functions produce SQL fragments and parameter values that callers
//! compose into their own `sqlx::query` calls. They deliberately do **not**
//! build complete queries — the surrounding `SELECT`, `FROM`, `WHERE`, and
//! result-mapping belong to the caller.
//!
//! # Examples
//!
//! ```
//! use klauthed_data::pagination::{OffsetPageRequest, SortKey};
//! use klauthed_data::pagination::sql::{limit_offset, sort_clause};
//!
//! let req = OffsetPageRequest::new(2, 20).unwrap();
//! let (limit, offset) = limit_offset(&req);
//! assert_eq!(limit, 20);
//! assert_eq!(offset, 20); // page 2, 20-per-page → offset 20
//!
//! let order = sort_clause(
//!     &[SortKey::asc("created_at"), SortKey::desc("id")],
//!     &["created_at", "id", "name"],
//! );
//! assert_eq!(order, " ORDER BY created_at ASC, id DESC");
//! ```

use super::{OffsetPageRequest, SortKey, SortOrder};

/// Re-exported so the keyset SQL helpers can be imported from one place
/// alongside [`keyset_where_clause`].
pub use super::KeysetDialect;

/// Return `(limit, offset)` as `i64` values ready to bind to a parameterised
/// query:
///
/// ```no_run
/// # use klauthed_data::pagination::OffsetPageRequest;
/// # use klauthed_data::pagination::sql::limit_offset;
/// # async fn run(pool: sqlx::AnyPool, req: OffsetPageRequest) -> Result<(), sqlx::Error> {
/// let (limit, offset) = limit_offset(&req);
/// let rows = sqlx::query("SELECT * FROM users LIMIT $1 OFFSET $2")
///     .bind(limit)
///     .bind(offset)
///     .fetch_all(&pool)
///     .await?;
/// # Ok(())
/// # }
/// ```
pub fn limit_offset(req: &OffsetPageRequest) -> (i64, i64) {
    (req.limit() as i64, req.offset() as i64)
}

/// Build an `ORDER BY` clause from `sort`, filtering to only fields in
/// `allowed_fields`.
///
/// Unknown fields are **silently dropped** to prevent SQL injection — the
/// sort column names are placed directly into the SQL string (they cannot be
/// bound as parameters). Always supply a restrictive `allowed_fields` list.
///
/// Returns an empty string when `sort` is empty or all fields are filtered
/// out, so the caller's default ordering applies.
///
/// ```
/// use klauthed_data::pagination::{SortKey, SortOrder};
/// use klauthed_data::pagination::sql::sort_clause;
///
/// let clause = sort_clause(
///     &[SortKey::asc("name"), SortKey::desc("created_at")],
///     &["name", "created_at", "id"],
/// );
/// assert_eq!(clause, " ORDER BY name ASC, created_at DESC");
///
/// // Unknown fields are dropped.
/// let clause = sort_clause(
///     &[SortKey::asc("__proto__")],
///     &["name", "id"],
/// );
/// assert_eq!(clause, "");
///
/// // Empty sort → empty string.
/// assert_eq!(sort_clause(&[], &["id"]), "");
/// ```
pub fn sort_clause(sort: &[SortKey], allowed_fields: &[&str]) -> String {
    let parts: Vec<String> = sort
        .iter()
        .filter(|k| allowed_fields.contains(&k.field.as_str()))
        .map(|k| {
            let dir = match k.order {
                SortOrder::Asc => "ASC",
                SortOrder::Desc => "DESC",
            };
            format!("{} {}", k.field, dir)
        })
        .collect();

    if parts.is_empty() { String::new() } else { format!(" ORDER BY {}", parts.join(", ")) }
}

/// Build the keyset `WHERE` clause fragment for compound column pagination.
///
/// Generates `(col1, col2) > ($1, $2)` (positional dialect) or
/// `(col1, col2) > (?, ?)` (question-mark dialect) for the given `sort` keys,
/// starting bind parameters at `start_param`.
///
/// **Security:** column names are taken from `sort` without quoting and must
/// have been validated against an allowed-fields list before calling this.
///
/// Returns `None` when `sort` is empty (no keyset condition needed).
///
/// ```
/// use klauthed_data::pagination::SortKey;
/// use klauthed_data::pagination::sql::{KeysetDialect, keyset_where_clause};
///
/// let clause = keyset_where_clause(
///     &[SortKey::asc("created_at"), SortKey::asc("id")],
///     1,
///     KeysetDialect::Positional,
/// );
/// assert_eq!(clause, Some("(created_at, id) > ($1, $2)".to_owned()));
///
/// let clause = keyset_where_clause(
///     &[SortKey::desc("score"), SortKey::asc("id")],
///     1,
///     KeysetDialect::Question,
/// );
/// assert_eq!(clause, Some("(score, id) > (?, ?)".to_owned()));
///
/// assert_eq!(keyset_where_clause(&[], 1, KeysetDialect::Positional), None);
/// ```
pub fn keyset_where_clause(
    sort: &[SortKey],
    start_param: usize,
    dialect: KeysetDialect,
) -> Option<String> {
    if sort.is_empty() {
        return None;
    }
    let cols: Vec<&str> = sort.iter().map(|k| k.field.as_str()).collect();
    let params: Vec<String> = (0..sort.len())
        .map(|i| match dialect {
            KeysetDialect::Positional => format!("${}", start_param + i),
            KeysetDialect::Question => "?".to_owned(),
        })
        .collect();
    Some(format!("({}) > ({})", cols.join(", "), params.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pagination::{OffsetPageRequest, SortKey};

    #[test]
    fn limit_offset_page1() {
        let req = OffsetPageRequest::new(1, 20).unwrap();
        assert_eq!(limit_offset(&req), (20, 0));
    }

    #[test]
    fn limit_offset_page3() {
        let req = OffsetPageRequest::new(3, 10).unwrap();
        assert_eq!(limit_offset(&req), (10, 20));
    }

    #[test]
    fn sort_clause_single_asc() {
        let s = sort_clause(&[SortKey::asc("name")], &["name", "id"]);
        assert_eq!(s, " ORDER BY name ASC");
    }

    #[test]
    fn sort_clause_multi_mixed() {
        let s =
            sort_clause(&[SortKey::asc("created_at"), SortKey::desc("id")], &["created_at", "id"]);
        assert_eq!(s, " ORDER BY created_at ASC, id DESC");
    }

    #[test]
    fn sort_clause_filters_unknown_fields() {
        let s = sort_clause(&[SortKey::asc("evil; DROP TABLE users")], &["name"]);
        assert_eq!(s, "");
    }

    #[test]
    fn sort_clause_empty_returns_empty_string() {
        assert_eq!(sort_clause(&[], &["id"]), "");
    }

    #[test]
    fn keyset_where_positional() {
        let clause = keyset_where_clause(
            &[SortKey::asc("created_at"), SortKey::asc("id")],
            1,
            KeysetDialect::Positional,
        );
        assert_eq!(clause, Some("(created_at, id) > ($1, $2)".to_owned()));
    }

    #[test]
    fn keyset_where_question_mark() {
        let clause = keyset_where_clause(&[SortKey::asc("id")], 1, KeysetDialect::Question);
        assert_eq!(clause, Some("(id) > (?)".to_owned()));
    }

    #[test]
    fn keyset_where_offset_start_param() {
        let clause = keyset_where_clause(
            &[SortKey::asc("a"), SortKey::desc("b")],
            3,
            KeysetDialect::Positional,
        );
        assert_eq!(clause, Some("(a, b) > ($3, $4)".to_owned()));
    }

    #[test]
    fn keyset_where_empty_sort_is_none() {
        assert_eq!(keyset_where_clause(&[], 1, KeysetDialect::Positional), None);
    }
}
