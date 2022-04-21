use proc_macro::TokenStream;
use syn::Error;

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

fn errors_to_tokenstream(errors: Vec<Error>) -> TokenStream {
    errors
        .into_iter()
        .reduce(|mut e, e2| {
            e.combine(e2);
            e
        })
        .map(|e| e.into_compile_error().into())
        .unwrap_or_default()
}
