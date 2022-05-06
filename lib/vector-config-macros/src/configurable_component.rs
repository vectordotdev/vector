use core::fmt;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{
    parse_macro_input, spanned::Spanned, AttributeArgs, DeriveInput, Error, Meta, NestedMeta,
};

enum ComponentType {
    Source,
    Transform,
    Sink,
}

impl ComponentType {
    pub fn try_from<S: fmt::Display>(input: S) -> Option<ComponentType> {
        let s = input.to_string();
        match s.as_str() {
            "source" => Some(Self::Source),
            "transform" => Some(Self::Transform),
            "sink" => Some(Self::Sink),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Transform => "transform",
            Self::Sink => "sink",
        }
    }
}

pub fn configurable_component_impl(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as AttributeArgs);
    let component_type = match find_component_type(&args) {
        Ok(ct) => ct
            .map(|ct| ct.as_str().to_string())
            .map(|s| quote! { #[configurable(metadata(component_type = #s))] })
            .unwrap_or_default(),
        Err(e) => return e.into_compile_error().into(),
    };

    let input = parse_macro_input!(item as DeriveInput);
    let derived = quote! {
        #[derive(::vector_config_macros::Configurable, ::serde::Serialize, ::serde::Deserialize)]
        #component_type
        #input
    };

    derived.into()
}

fn find_component_type(args: &AttributeArgs) -> Result<Option<ComponentType>, Error> {
    // Try parsing out the component type from the list of arguments.  In most cases, this will only
    // ever be a single item and it will be valid, but since attribute macros can support many
    // arguments, and many argument types, we need to handle all of those cases and emit a useful
    // error message that tells the user the way in which they've messed up, and what they should do.
    let mut errors = Vec::new();

    let mut component_type = args.iter()
        .filter_map(|nm| match nm {
            NestedMeta::Meta(Meta::Path(p)) => match p.get_ident().and_then(ComponentType::try_from) {
                Some(ct) => Some(ct),
                None => {
                    errors.push(Error::new(nm.span(), "unknown argument for `configurable_component`; valid options are `source`, `transform`, and `sink`"));
                    None
                },
            },
            nm => {
                errors.push(Error::new(nm.span(), "unknown argument type for `configurable_component`"));
                None
            },
        })
        .collect::<Vec<_>>();

    // If we encountered any errors during argument filtering, just return now:
    if !errors.is_empty() {
        return Err(errors
            .into_iter()
            .reduce(|mut e, e2| {
                e.combine(e2);
                e
            })
            .unwrap());
    }

    // There should only ever be a single component type defined:
    if component_type.len() >= 2 {
        return Err(Error::new(
            Span::call_site(),
            "specifying multiple component types is not allowed",
        ));
    }

    Ok(component_type.pop())
}
