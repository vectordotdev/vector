use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemStruct};

// #[internal_event] attribute macro implementation.
//
// Apply to a struct that also implements `InternalEvent`. This generates an
// implementation of `NamedInternalEvent` so `InternalEvent::name()` returns
// a canonical &'static str for the event's type.
pub fn internal_event_impl(item: TokenStream) -> TokenStream {
    let item_struct = parse_macro_input!(item as ItemStruct);

    let ident = &item_struct.ident;
    let generics = item_struct.generics.clone();
    let (impl_generics, ty_generics, where_clause2) = generics.split_for_impl();

    // Use a path that works from both vector-common (crate::internal_event)
    // and from other crates using vector-lib (vector_lib::internal_event).
    // For crates that don't depend on vector-lib but do depend on vector-common,
    // we use vector_common::internal_event.
    let pkg_name = std::env::var("CARGO_PKG_NAME").unwrap_or_default();
    let internal_event_path = if pkg_name == "vector-common" {
        quote! { crate::internal_event }
    } else if pkg_name == "vector-buffers" || pkg_name.starts_with("vector-") {
        // Most vector-* crates depend on vector-common but not vector-lib
        quote! { ::vector_common::internal_event }
    } else {
        // Main vector crate and its internal modules use vector_lib
        quote! { ::vector_lib::internal_event }
    };

    let expanded = quote! {
        #item_struct

        impl #impl_generics #internal_event_path::NamedInternalEvent for #ident #ty_generics #where_clause2 {
            fn name(&self) -> &'static str { stringify!(#ident) }
        }
    };

    TokenStream::from(expanded)
}
