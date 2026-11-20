mod commands;
mod events;
mod listeners;
mod module;
mod plugin;

use proc_macro::TokenStream;
use syn::{DeriveInput, Error, ItemImpl, parse_macro_input};

#[proc_macro_derive(Module, attributes(module))]
pub fn derive_module(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    module::derive(&input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

#[proc_macro_derive(Plugin, attributes(plugin, module))]
pub fn derive_plugin(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    plugin::derive(&input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

#[proc_macro_attribute]
pub fn listeners(args: TokenStream, input: TokenStream) -> TokenStream {
    let block = parse_macro_input!(input as ItemImpl);
    listeners::expand(args.into(), block).into()
}

#[proc_macro_attribute]
pub fn command(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input = proc_macro2::TokenStream::from(input);
    let error = Error::new_spanned(
        &input,
        "#[command] goes on a method inside a #[listeners] impl block",
    );
    let mut output = error.into_compile_error();
    output.extend(input);
    output.into()
}
