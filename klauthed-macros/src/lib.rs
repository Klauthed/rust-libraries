//! Procedural macros for klauthed.
//!
//! Currently: [`macro@DomainError`], a derive that generates the
//! `klauthed_error::DomainError` impl from `#[domain(...)]` attributes, so error
//! types don't hand-write the `category()` / `code()` match arms.

use heck::ToSnakeCase as _;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::spanned::Spanned;
use syn::{Attribute, Data, DeriveInput, Fields, Ident, LitStr, parse_macro_input};

/// Derive `klauthed_error::DomainError`.
///
/// Annotate variants (or a struct) with `#[domain(...)]`:
///
/// ## Compile-time validation
///
/// Both `code` and `prefix` are validated at macro-expansion time:
/// they must match `[a-z][a-z0-9_]*` (plus dots in `code` for fully-qualified
/// codes). Violations are hard compile errors, not silent runtime bugs.
///
/// ```compile_fail
/// # use klauthed_macros::DomainError;
/// #[derive(Debug, DomainError)]
/// #[domain(prefix = "BadPrefix")]  // uppercase → compile error
/// enum Bad { A }
/// # impl std::fmt::Display for Bad { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) } }
/// # impl std::error::Error for Bad {}
/// ```
///
/// ```compile_fail
/// # use klauthed_macros::DomainError;
/// #[derive(Debug, DomainError)]
/// #[domain(prefix = "my")]
/// enum Bad {
///     #[domain(code = "bad code with spaces")]  // spaces → compile error
///     A,
/// }
/// # impl std::fmt::Display for Bad { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) } }
/// # impl std::error::Error for Bad {}
/// ```
///
/// * `category = "internal"` — one of the snake-case `ErrorCategory` names
///   (`bad_request`, `unauthorized`, `forbidden`, `not_found`, `conflict`,
///   `rate_limited`, `timeout`, `unavailable`, `internal`). Defaults to the
///   container's `category`, else `internal`.
/// * `code = "missing"` — the error code. Defaults to the snake-cased variant
///   name. A container `#[domain(prefix = "config")]` prefixes every code, so
///   `MissingRequired` → `config.missing_required`.
/// * `transparent` — delegate `category()` and `code()` to the variant's single
///   field (which must itself be a `DomainError`), for wrapped/`#[from]` errors.
///
/// The type must also implement `std::error::Error` (e.g. via `thiserror`),
/// since `DomainError` requires it.
#[proc_macro_derive(DomainError, attributes(domain))]
pub fn derive_domain_error(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Parsed `#[domain(...)]` options (used at both container and variant level).
#[derive(Default)]
struct DomainAttr {
    category: Option<String>,
    code: Option<String>,
    prefix: Option<String>,
    transparent: bool,
}

fn parse_domain_attr(attrs: &[Attribute]) -> syn::Result<DomainAttr> {
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
            let is_code = meta.path.is_ident("code");
            let is_prefix = meta.path.is_ident("prefix");
            let target = if meta.path.is_ident("category") {
                &mut out.category
            } else if is_code {
                &mut out.code
            } else if is_prefix {
                &mut out.prefix
            } else {
                return Err(meta
                    .error("unknown `domain` key (expected category, code, prefix, or transparent)"));
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
            format!("code '{value}' has invalid dot placement (no leading, trailing, or consecutive dots)"),
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
fn validate_ident_segment(
    segment: &str,
    full: &str,
    span: proc_macro2::Span,
) -> syn::Result<()> {
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

fn expand(input: DeriveInput) -> syn::Result<TokenStream2> {
    let container = parse_domain_attr(&input.attrs)?;
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let (category_body, code_body) = match &input.data {
        Data::Enum(data) => enum_bodies(&data.variants, &container)?,
        Data::Struct(data) => struct_bodies(name, &data.fields, &container)?,
        Data::Union(_) => {
            return Err(syn::Error::new(
                input.span(),
                "DomainError cannot be derived for unions",
            ));
        }
    };

    Ok(quote! {
        impl #impl_generics ::klauthed_error::DomainError for #name #ty_generics #where_clause {
            fn category(&self) -> ::klauthed_error::ErrorCategory {
                #category_body
            }
            fn code(&self) -> ::klauthed_error::ErrorCode {
                #code_body
            }
        }
    })
}

fn enum_bodies<'a>(
    variants: impl IntoIterator<Item = &'a syn::Variant>,
    container: &DomainAttr,
) -> syn::Result<(TokenStream2, TokenStream2)> {
    let mut category_arms = Vec::new();
    let mut code_arms = Vec::new();

    for variant in variants {
        let attr = parse_domain_attr(&variant.attrs)?;
        let vident = &variant.ident;
        // Forward `#[cfg(...)]` so feature-gated variants and their match arms
        // appear or vanish together.
        let cfgs: Vec<&Attribute> = variant
            .attrs
            .iter()
            .filter(|a| a.path().is_ident("cfg"))
            .collect();

        if attr.transparent {
            let (pattern, binding) = transparent_pattern(&variant.fields, vident.span())?;
            category_arms.push(quote! {
                #(#cfgs)* Self::#vident #pattern => ::klauthed_error::DomainError::category(#binding),
            });
            code_arms.push(quote! {
                #(#cfgs)* Self::#vident #pattern => ::klauthed_error::DomainError::code(#binding),
            });
        } else {
            let category = category_path(&attr, container, vident.span())?;
            let code = build_code(container.prefix.as_deref(), attr.code.as_deref(), vident);
            let pattern = ignore_pattern(&variant.fields);
            category_arms.push(quote! { #(#cfgs)* Self::#vident #pattern => #category, });
            code_arms.push(
                quote! { #(#cfgs)* Self::#vident #pattern => ::klauthed_error::ErrorCode::new(#code), },
            );
        }
    }

    let category_body = quote! { match self { #(#category_arms)* } };
    let code_body = quote! { match self { #(#code_arms)* } };
    Ok((category_body, code_body))
}

fn struct_bodies(
    name: &Ident,
    fields: &Fields,
    container: &DomainAttr,
) -> syn::Result<(TokenStream2, TokenStream2)> {
    if container.transparent {
        let access = transparent_field_access(fields, name.span())?;
        let category_body = quote! { ::klauthed_error::DomainError::category(&#access) };
        let code_body = quote! { ::klauthed_error::DomainError::code(&#access) };
        return Ok((category_body, code_body));
    }

    let category = category_path(container, container, name.span())?;
    let code = build_code(container.prefix.as_deref(), container.code.as_deref(), name);
    let category_body = quote! { #category };
    let code_body = quote! { ::klauthed_error::ErrorCode::new(#code) };
    Ok((category_body, code_body))
}

/// The `ErrorCategory::X` path for a variant, honoring variant → container →
/// `internal` precedence.
fn category_path(
    attr: &DomainAttr,
    container: &DomainAttr,
    span: proc_macro2::Span,
) -> syn::Result<TokenStream2> {
    let name = attr
        .category
        .as_deref()
        .or(container.category.as_deref())
        .unwrap_or("internal");
    let variant = match name {
        "bad_request" => "BadRequest",
        "unauthorized" => "Unauthorized",
        "forbidden" => "Forbidden",
        "not_found" => "NotFound",
        "unprocessable_entity" => "UnprocessableEntity",
        "conflict" => "Conflict",
        "rate_limited" => "RateLimited",
        "timeout" => "Timeout",
        "unavailable" => "Unavailable",
        "internal" => "Internal",
        other => {
            return Err(syn::Error::new(
                span,
                format!(
                    "unknown category '{other}' (expected bad_request, unauthorized, forbidden, \
                     not_found, unprocessable_entity, conflict, rate_limited, timeout, \
                     unavailable, or internal)"
                ),
            ));
        }
    };
    let ident = Ident::new(variant, span);
    Ok(quote! { ::klauthed_error::ErrorCategory::#ident })
}

/// Build the final code string: `{prefix}.{code|snake(ident)}`, or just the
/// code/snaked-ident when no prefix is set.
///
/// Conversion uses [`heck::ToSnakeCase`], which correctly handles consecutive
/// capitals (e.g. `HTTPError` → `http_error`, `APIKey` → `api_key`).
fn build_code(prefix: Option<&str>, code: Option<&str>, ident: &Ident) -> String {
    let suffix = code
        .map(str::to_owned)
        .unwrap_or_else(|| ident.to_string().to_snake_case());
    match prefix {
        Some(prefix) => format!("{prefix}.{suffix}"),
        None => suffix,
    }
}

/// A field-ignoring pattern for a variant: ``, `(..)`, or `{ .. }`.
fn ignore_pattern(fields: &Fields) -> TokenStream2 {
    match fields {
        Fields::Unit => quote! {},
        Fields::Unnamed(_) => quote! { (..) },
        Fields::Named(_) => quote! { { .. } },
    }
}

/// Pattern + binding for a transparent enum variant (must have exactly one field).
fn transparent_pattern(
    fields: &Fields,
    span: proc_macro2::Span,
) -> syn::Result<(TokenStream2, TokenStream2)> {
    match fields {
        Fields::Unnamed(f) if f.unnamed.len() == 1 => {
            Ok((quote! { (__inner) }, quote! { __inner }))
        }
        Fields::Named(f) if f.named.len() == 1 => {
            let field = f.named.first().unwrap().ident.as_ref().unwrap();
            Ok((quote! { { #field: __inner } }, quote! { __inner }))
        }
        _ => Err(syn::Error::new(
            span,
            "`#[domain(transparent)]` requires the variant to have exactly one field",
        )),
    }
}

/// Field access (`self.0` / `self.field`) for a transparent newtype struct.
fn transparent_field_access(fields: &Fields, span: proc_macro2::Span) -> syn::Result<TokenStream2> {
    match fields {
        Fields::Unnamed(f) if f.unnamed.len() == 1 => Ok(quote! { self.0 }),
        Fields::Named(f) if f.named.len() == 1 => {
            let field = f.named.first().unwrap().ident.as_ref().unwrap();
            Ok(quote! { self.#field })
        }
        _ => Err(syn::Error::new(
            span,
            "`#[domain(transparent)]` requires the struct to have exactly one field",
        )),
    }
}

