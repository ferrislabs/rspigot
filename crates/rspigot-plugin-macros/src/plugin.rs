use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, Error, LitStr};

use crate::module;

pub(crate) fn derive(input: &DeriveInput) -> Result<TokenStream, Error> {
    let mut name = None;
    let mut version = None;
    for attr in input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("plugin"))
    {
        attr.parse_nested_meta(|meta| {
            let (key, slot) = if meta.path.is_ident("name") {
                ("name", &mut name)
            } else if meta.path.is_ident("version") {
                ("version", &mut version)
            } else if meta.path.is_ident("own") {
                return Ok(());
            } else {
                return Err(meta.error(
                    "`#[plugin(...)]` takes `name = \"...\"`, `version = \"...\"` and `own`",
                ));
            };
            if slot.is_some() {
                return Err(meta.error(format_args!("the plugin's `{key}` is already set")));
            }
            *slot = Some(meta.value()?.parse::<LitStr>()?);
            Ok(())
        })?;
    }
    let name = name.map_or_else(
        || quote!(::core::env!("CARGO_PKG_NAME")),
        |name| quote!(#name),
    );
    let version = version.map_or_else(
        || quote!(::core::env!("CARGO_PKG_VERSION")),
        |version| quote!(#version),
    );
    let plugin = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    let own = module::own_attribute(input, "plugin")?;
    let fields = module::fields(input)?;
    let composed = (!fields.is_empty()).then(|| module::expand(input, &fields, own));
    Ok(quote! {
        #composed

        #[automatically_derived]
        impl #impl_generics ::rspigot_plugin::Plugin for #plugin #type_generics #where_clause {
            const NAME: &'static str = #name;
            const VERSION: &'static str = #version;
        }
    })
}
