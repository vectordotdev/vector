// TODO: Remove this once we add validation since that's the only piece of dead code in this crate at the moment.
#![allow(dead_code)]

use proc_macro::TokenStream;

mod ast;
mod configurable;
mod configurable_component;

/// Allows the given struct/enum to be used as a type within the Vector configuration.
#[proc_macro_attribute]
pub fn configurable_component(args: TokenStream, item: TokenStream) -> TokenStream {
    configurable_component::configurable_component_impl(args, item)
}

#[proc_macro_derive(Configurable, attributes(configurable))]
pub fn derive_configurable(input: TokenStream) -> TokenStream {
    configurable::derive_configurable_impl(input)
}
