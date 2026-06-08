//! Expansion for `#[derive(FromConfig)]`.
//!
//! Generates an `impl ::klauthed_core::config::FromConfig` that reads the type
//! from a config key (the snake-cased type name by default, or an explicit
//! `#[config(key = "…")]`). With `#[config(default)]` a missing section binds to
//! `Default::default()` instead of erroring.

use heck::ToSnakeCase;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, LitStr};

/// Options parsed from the type's `#[config(...)]` attribute.
struct Options {
    key: Option<String>,
    default: bool,
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

    Ok(quote! {
        impl #impl_generics ::klauthed_core::config::FromConfig for #ident #ty_generics
            #where_clause
        {
            fn from_config(
                config: &::klauthed_core::config::Config,
            ) -> ::core::result::Result<Self, ::klauthed_core::error::ConfigError> {
                #body
            }
        }
    })
}

/// Parse the (optional) `#[config(key = "…", default)]` attribute.
fn parse_options(input: &DeriveInput) -> syn::Result<Options> {
    let mut options = Options { key: None, default: false };

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
            } else {
                Err(meta.error("unknown `config` option (expected `key` or `default`)"))
            }
        })?;
    }

    Ok(options)
}
