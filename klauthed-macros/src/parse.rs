//! Parsing and validation of the `#[domain(...)]` attribute.

use syn::{Attribute, LitStr, Path};

/// Parsed `#[domain(...)]` options (used at both container and variant level).
#[derive(Default)]
pub(crate) struct DomainAttr {
    pub(crate) category: Option<String>,
    pub(crate) code: Option<String>,
    pub(crate) prefix: Option<String>,
    pub(crate) transparent: bool,
    /// Path to the `klauthed_error` crate/module, for crates that reach it through
    /// a re-export (e.g. `crate = "klauthed::error"` when depending only on the
    /// `klauthed` umbrella). Container-level; defaults to `::klauthed_error`.
    pub(crate) krate: Option<Path>,
}

pub(crate) fn parse_domain_attr(attrs: &[Attribute]) -> syn::Result<DomainAttr> {
    let mut out = DomainAttr::default();
    for attr in attrs {
        if !attr.path().is_ident("domain") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("transparent") {
                out.transparent = true;
                return Ok(());
            }
            if meta.path.is_ident("crate") {
                // `crate = "klauthed::error"` — value is a string holding a path.
                let value: LitStr = meta.value()?.parse()?;
                out.krate = Some(value.parse()?);
                return Ok(());
            }
            let is_code = meta.path.is_ident("code");
            let is_prefix = meta.path.is_ident("prefix");
            let target = if meta.path.is_ident("category") {
                &mut out.category
            } else if is_code {
                &mut out.code
            } else if is_prefix {
                &mut out.prefix
            } else {
                return Err(meta.error(
                    "unknown `domain` key (expected category, code, prefix, transparent, or crate)",
                ));
            };
            let value: LitStr = meta.value()?.parse()?;
            let s = value.value();
            // Validate `code` and `prefix` at compile time so typos surface as
            // clear errors rather than garbled runtime codes.
            if is_code {
                validate_code_segment(&s, value.span())?;
            } else if is_prefix {
                validate_prefix_segment(&s, value.span())?;
            }
            *target = Some(s);
            Ok(())
        })?;
    }
    Ok(out)
}

/// Validate a `code` attribute value.
///
/// A `code` may be a bare suffix (`"missing_required"`) or a fully-qualified
/// code (`"upstream.down"`) when no container `prefix` is set. Each
/// dot-separated segment must be `[a-z][a-z0-9_]*`. Leading / trailing /
/// consecutive dots are rejected.
fn validate_code_segment(value: &str, span: proc_macro2::Span) -> syn::Result<()> {
    if value.is_empty() {
        return Err(syn::Error::new(span, "code must not be empty"));
    }
    if value.starts_with('.') || value.ends_with('.') || value.contains("..") {
        return Err(syn::Error::new(
            span,
            format!(
                "code '{value}' has invalid dot placement (no leading, trailing, or consecutive dots)"
            ),
        ));
    }
    for segment in value.split('.') {
        validate_ident_segment(segment, value, span)?;
    }
    Ok(())
}

/// Validate a `prefix` attribute value.
///
/// A prefix is a single namespace segment with **no dots** —
/// the full code is built as `{prefix}.{code}`. Must be `[a-z][a-z0-9_]*`.
fn validate_prefix_segment(value: &str, span: proc_macro2::Span) -> syn::Result<()> {
    if value.is_empty() {
        return Err(syn::Error::new(span, "prefix must not be empty"));
    }
    if value.contains('.') {
        return Err(syn::Error::new(
            span,
            format!("prefix '{value}' must not contain dots — it is a single namespace label"),
        ));
    }
    validate_ident_segment(value, value, span)
}

/// Validate a single `[a-z][a-z0-9_]*` identifier segment.
fn validate_ident_segment(segment: &str, full: &str, span: proc_macro2::Span) -> syn::Result<()> {
    if segment.is_empty() {
        return Err(syn::Error::new(
            span,
            format!("code/prefix '{full}' contains an empty segment"),
        ));
    }
    let mut chars = segment.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        Some(other) => {
            return Err(syn::Error::new(
                span,
                format!(
                    "'{segment}' must start with a lowercase ASCII letter (a-z), \
                     not '{other}'"
                ),
            ));
        }
        None => unreachable!(),
    }
    for c in chars {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '_' {
            return Err(syn::Error::new(
                span,
                format!(
                    "'{segment}' contains invalid character '{c}' — \
                     only lowercase letters (a-z), digits (0-9), and underscores (_) are allowed"
                ),
            ));
        }
    }
    Ok(())
}
