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

    let expanded = quote! {
        #item_struct

        impl #impl_generics ::vector_lib::internal_event::NamedInternalEvent for #ident #ty_generics #where_clause2 {
            fn name(&self) -> &'static str { stringify!(#ident) }
        }
    };

    TokenStream::from(expanded)
}
