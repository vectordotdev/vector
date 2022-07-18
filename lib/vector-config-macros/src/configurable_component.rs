use darling::{util::Flag, FromMeta};
use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, parse_quote, punctuated::Punctuated, token::Comma, AttributeArgs,
    DeriveInput, Path,
};

enum ComponentType {
    Source,
    Transform,
    Sink,
}

impl ComponentType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Transform => "transform",
            Self::Sink => "sink",
        }
    }
}

#[derive(Debug, FromMeta)]
struct Options {
    #[darling(default)]
    source: Flag,
    #[darling(default)]
    transform: Flag,
    #[darling(default)]
    sink: Flag,
    #[darling(default)]
    no_ser: Flag,
    #[darling(default)]
    no_deser: Flag,
}

impl Options {
    fn component_type(&self) -> Option<ComponentType> {
        if self.source.is_some() {
            return Some(ComponentType::Source);
        }

        if self.transform.is_some() {
            return Some(ComponentType::Transform);
        }

        if self.sink.is_some() {
            return Some(ComponentType::Sink);
        }

        None
    }

    fn should_derive_ser(&self) -> bool {
        self.no_ser.is_none()
    }

    fn should_derive_deser(&self) -> bool {
        self.no_deser.is_none()
    }
}

pub fn configurable_component_impl(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as AttributeArgs);
    let options = match Options::from_list(&args) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(e.write_errors());
        }
    };

    let component_type = options
        .component_type()
        .map(|ct| ct.as_str().to_string())
        .map(|s| quote! { #[configurable(metadata(component_type = #s))] });

    let mut derives = Punctuated::<Path, Comma>::new();
    derives.push(parse_quote! { ::vector_config_macros::Configurable });

    if options.should_derive_ser() {
        derives.push(parse_quote! { ::serde::Serialize });
    }

    if options.should_derive_deser() {
        derives.push(parse_quote! { ::serde::Deserialize });
    }

    let input = parse_macro_input!(item as DeriveInput);
    let derived = quote! {
        #[derive(#derives)]
        #component_type
        #input
    };

    derived.into()
}
