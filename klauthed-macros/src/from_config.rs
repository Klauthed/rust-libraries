//! Expansion for `#[derive(FromConfig)]`.
//!
//! Generates an `impl ::klauthed_core::config::FromConfig` that reads the type
//! from a config key (the snake-cased type name by default, or an explicit
//! `#[config(key = "…")]`). With `#[config(default)]` a missing section binds to
//! `Default::default()` instead of erroring.
//!
//! `#[config(crate = "path")]` points the generated impl at the `klauthed_core`
//! crate/module via a re-export (e.g. `crate = "klauthed::core"` for a crate that
//! depends only on the `klauthed` umbrella). Defaults to `::klauthed_core`.

use heck::ToSnakeCase;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, LitStr, Path};

/// Options parsed from the type's `#[config(...)]` attribute.
struct Options {
    key: Option<String>,
    default: bool,
    krate: Option<Path>,
}

/// Expand `#[derive(FromConfig)]` for `input`.
pub fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    if !matches!(input.data, Data::Struct(_)) {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "FromConfig can only be derived for structs",
        ));
    }

    let options = parse_options(&input)?;
    let ident = &input.ident;
    let key = options.key.unwrap_or_else(|| ident.to_string().to_snake_case());
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let body = if options.default {
        quote! {
            ::core::result::Result::Ok(
                config.get_optional::<Self>(#key)?.unwrap_or_default()
            )
        }
    } else {
        quote! { config.get::<Self>(#key) }
    };

    // The core crate is reachable as `__klauthed_core` inside the generated block.
    // Defaults to the `klauthed_core` crate; `#[config(crate = "…")]` overrides it
    // (e.g. `"klauthed::core"` for umbrella-only crates). The `use` + `const _`
    // wrapper keeps the alias hygienic.
    let krate: Path = options.krate.clone().unwrap_or_else(|| syn::parse_quote!(::klauthed_core));

    Ok(quote! {
        const _: () = {
            use #krate as __klauthed_core;
            impl #impl_generics __klauthed_core::config::FromConfig for #ident #ty_generics
                #where_clause
            {
                fn from_config(
                    config: &__klauthed_core::config::Config,
                ) -> ::core::result::Result<Self, __klauthed_core::error::ConfigError> {
                    #body
                }
            }
        };
    })
}

/// Parse the (optional) `#[config(key = "…", default, crate = "…")]` attribute.
fn parse_options(input: &DeriveInput) -> syn::Result<Options> {
    let mut options = Options { key: None, default: false, krate: None };

    for attr in &input.attrs {
        if !attr.path().is_ident("config") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("key") {
                let lit: LitStr = meta.value()?.parse()?;
                options.key = Some(lit.value());
                Ok(())
            } else if meta.path.is_ident("default") {
                options.default = true;
                Ok(())
            } else if meta.path.is_ident("crate") {
                // `crate = "klauthed::core"` — value is a string holding a path.
                let lit: LitStr = meta.value()?.parse()?;
                options.krate = Some(lit.parse()?);
                Ok(())
            } else {
                Err(meta.error("unknown `config` option (expected `key`, `default`, or `crate`)"))
            }
        })?;
    }

    Ok(options)
}
