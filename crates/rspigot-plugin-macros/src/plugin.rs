use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, Error, Expr, ExprArray, ExprLit, Lit, LitStr};

use crate::module;

pub(crate) fn derive(input: &DeriveInput) -> Result<TokenStream, Error> {
    let mut name = None;
    let mut version = None;
    let mut depends: Option<Vec<LitStr>> = None;
    let mut soft_depends: Option<Vec<LitStr>> = None;
    let mut load_before: Option<Vec<LitStr>> = None;
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
            } else if meta.path.is_ident("depends") {
                return names(&meta, "depends", &mut depends);
            } else if meta.path.is_ident("soft_depends") {
                return names(&meta, "soft_depends", &mut soft_depends);
            } else if meta.path.is_ident("load_before") {
                return names(&meta, "load_before", &mut load_before);
            } else {
                return Err(meta.error(
                    "`#[plugin(...)]` takes `name = \"...\"`, `version = \"...\"`, `depends = [\"...\"]`, `soft_depends = [\"...\"]`, `load_before = [\"...\"]` and `own`",
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
    let relations = [
        ("DEPENDS", depends),
        ("SOFT_DEPENDS", soft_depends),
        ("LOAD_BEFORE", load_before),
    ]
    .into_iter()
    .filter_map(|(constant, list)| {
        list.map(|list| {
            let constant = syn::Ident::new(constant, proc_macro2::Span::call_site());
            quote!(const #constant: &'static [&'static str] = &[#(#list),*];)
        })
    });
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
            #(#relations)*
        }
    })
}

fn names(
    meta: &syn::meta::ParseNestedMeta<'_>,
    key: &str,
    slot: &mut Option<Vec<LitStr>>,
) -> Result<(), Error> {
    if slot.is_some() {
        return Err(meta.error(format_args!("the plugin's `{key}` is already set")));
    }
    let array: ExprArray = meta.value()?.parse()?;
    let mut list = Vec::new();
    for element in array.elems {
        match element {
            Expr::Lit(ExprLit {
                lit: Lit::Str(name),
                ..
            }) => list.push(name),
            other => {
                return Err(Error::new_spanned(
                    other,
                    format!("`{key}` is a list of plugin names: `{key} = [\"name\"]`"),
                ));
            }
        }
    }
    *slot = Some(list);
    Ok(())
}
