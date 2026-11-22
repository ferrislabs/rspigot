use proc_macro2::TokenStream;
use quote::quote;
use syn::{Error, Fields, ItemStruct};

pub(crate) fn expand(args: TokenStream, item: ItemStruct) -> TokenStream {
    let mut errors = Vec::new();
    if !args.is_empty() {
        errors.push(Error::new_spanned(&args, "#[config] takes no arguments"));
    }
    let Fields::Named(named) = &item.fields else {
        errors.push(Error::new_spanned(
            &item.ident,
            "#[config] goes on a struct with named fields, one per key of `config.toml`",
        ));
        let errors = errors.iter().map(Error::to_compile_error);
        return quote!(#item #(#errors)*);
    };
    let name = &item.ident;
    let (impl_generics, type_generics, where_clause) = item.generics.split_for_impl();
    let reads = named.named.iter().filter_map(|field| {
        let ident = field.ident.as_ref()?;
        let key = ident.to_string();
        let ty = &field.ty;
        Some(quote! {
            #ident: ::rspigot_plugin::read_field::<#ty>(config, #key)?,
        })
    });
    let errors = errors.iter().map(Error::to_compile_error);
    quote! {
        #item

        #[automatically_derived]
        impl #impl_generics ::rspigot_plugin::FromConfig for #name #type_generics #where_clause {
            fn from_config(
                config: &::rspigot_plugin::Config,
            ) -> ::core::result::Result<Self, ::rspigot_plugin::ConfigError> {
                ::core::result::Result::Ok(Self {
                    #(#reads)*
                })
            }
        }

        #[automatically_derived]
        impl #impl_generics #name #type_generics #where_clause {
            pub fn load(
                plugin: &::core::primitive::str,
            ) -> ::core::result::Result<Self, ::rspigot_plugin::ConfigError> {
                ::rspigot_plugin::load_typed_for::<Self>(plugin)
            }

            pub fn load_from(
                folder: impl ::core::convert::Into<::std::path::PathBuf>,
            ) -> ::core::result::Result<Self, ::rspigot_plugin::ConfigError> {
                ::rspigot_plugin::load_typed::<Self>(folder)
            }
        }

        #(#errors)*
    }
}
