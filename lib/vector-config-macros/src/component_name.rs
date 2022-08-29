use proc_macro::TokenStream;

use quote::quote;
use syn::{parse_macro_input, DeriveInput, LitStr};

pub fn component_name_impl(attrs: TokenStream, item: TokenStream) -> TokenStream {
    // We only allow the `#[component_name("foobar")]` form, and so we can just try and parse the
    // attributes innards directly into a literal string. If it's anything else, it's not valid.
    let component_name = parse_macro_input!(attrs as LitStr);
    let input = parse_macro_input!(item as DeriveInput);

    let ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let modified = quote! {
        #input

        impl #impl_generics ::vector_config::NamedComponent for #ident #ty_generics #where_clause {
            const NAME: &'static str = #component_name;
        }
    };
    modified.into()
}
