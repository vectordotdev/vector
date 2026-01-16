use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, parse_macro_input, spanned::Spanned};

/// Implements `NamedInternalEvent` for structs via `#[derive(NamedInternalEvent)]`.
pub fn derive_impl_named_internal_event(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);

    if !matches!(input.data, Data::Struct(_)) {
        return syn::Error::new(
            input.span(),
            "#[derive(NamedInternalEvent)] can only be used with structs",
        )
        .to_compile_error()
        .into();
    }

    let DeriveInput {
        ident, generics, ..
    } = input;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Use a path that works from both vector-common (crate::internal_event)
    // and from other crates using vector-lib (vector_lib::internal_event).
    // For crates that don't depend on vector-lib but do depend on vector-common,
    // we use vector_common::internal_event.
    let pkg_name = std::env::var("CARGO_PKG_NAME").unwrap_or_default();
    let internal_event_path = if pkg_name == "vector-common" {
        quote! { crate::internal_event }
    } else if pkg_name.starts_with("vector-") || pkg_name == "dnstap-parser" {
        // Most vector-* crates depend on vector-common but not vector-lib
        quote! { ::vector_common::internal_event }
    } else {
        // Main vector crate and its internal modules use vector_lib
        quote! { ::vector_lib::internal_event }
    };

    let expanded = quote! {
        impl #impl_generics #internal_event_path::NamedInternalEvent for #ident #ty_generics #where_clause {
            fn name(&self) -> &'static str { stringify!(#ident) }
        }
    };

    TokenStream::from(expanded)
}
