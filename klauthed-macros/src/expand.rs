//! Code generation: expand a `DeriveInput` into the `DomainError` impl.

use heck::ToSnakeCase as _;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::spanned::Spanned;
use syn::{Attribute, Data, DeriveInput, Fields, Ident};

use crate::parse::{DomainAttr, parse_domain_attr};

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream2> {
    let container = parse_domain_attr(&input.attrs)?;
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let (category_body, code_body) = match &input.data {
        Data::Enum(data) => enum_bodies(&data.variants, &container)?,
        Data::Struct(data) => struct_bodies(name, &data.fields, &container)?,
        Data::Union(_) => {
            return Err(syn::Error::new(input.span(), "DomainError cannot be derived for unions"));
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
        let cfgs: Vec<&Attribute> =
            variant.attrs.iter().filter(|a| a.path().is_ident("cfg")).collect();

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
    let name = attr.category.as_deref().or(container.category.as_deref()).unwrap_or("internal");
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
    let suffix = code.map(str::to_owned).unwrap_or_else(|| ident.to_string().to_snake_case());
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
            #[allow(clippy::unwrap_used, reason = "the match guard proves exactly one named field")]
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
            #[allow(clippy::unwrap_used, reason = "the match guard proves exactly one named field")]
            let field = f.named.first().unwrap().ident.as_ref().unwrap();
            Ok(quote! { self.#field })
        }
        _ => Err(syn::Error::new(
            span,
            "`#[domain(transparent)]` requires the struct to have exactly one field",
        )),
    }
}
