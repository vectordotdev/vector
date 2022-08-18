use darling::{util::path_to_string, Error, FromMeta};
use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, parse_quote_spanned, punctuated::Punctuated, token::Comma, AttributeArgs,
    DeriveInput, Ident, Lit, Meta, NestedMeta, Path,
};

#[derive(Copy, Clone)]
struct AttributeIdent(&'static str);

const SOURCE: AttributeIdent = AttributeIdent("source");
const TRANSFORM: AttributeIdent = AttributeIdent("transform");
const SINK: AttributeIdent = AttributeIdent("sink");
const NO_SER: AttributeIdent = AttributeIdent("no_ser");
const NO_DESER: AttributeIdent = AttributeIdent("no_deser");

impl PartialEq<AttributeIdent> for Ident {
    fn eq(&self, word: &AttributeIdent) -> bool {
        self == word.0
    }
}

impl<'a> PartialEq<AttributeIdent> for &'a Ident {
    fn eq(&self, word: &AttributeIdent) -> bool {
        *self == word.0
    }
}

impl PartialEq<AttributeIdent> for Path {
    fn eq(&self, word: &AttributeIdent) -> bool {
        self.is_ident(word.0)
    }
}

impl<'a> PartialEq<AttributeIdent> for &'a Path {
    fn eq(&self, word: &AttributeIdent) -> bool {
        self.is_ident(word.0)
    }
}

fn path_any(path: &Path, needles: &[AttributeIdent]) -> bool {
    needles.iter().any(|ai| path == ai)
}

#[derive(Clone, Debug)]
enum ComponentType {
    Source(String),
    Transform(String),
    Sink(String),
}

impl ComponentType {
    fn as_type_str(&self) -> &'static str {
        match self {
            Self::Source(_) => "source",
            Self::Transform(_) => "transform",
            Self::Sink(_) => "sink",
        }
    }

    fn as_name_str(&self) -> &str {
        match self {
            Self::Source(s) => s.as_str(),
            Self::Transform(s) => s.as_str(),
            Self::Sink(s) => s.as_str(),
        }
    }
}

#[derive(Debug)]
struct Options {
    /// Component type, if specified.
    ///
    /// While the macro `#[configurable_component]` sort of belies an implication that the item
    /// being annotated is thus a component, we only consider sources, transforms, and sinks a true
    /// "component", in the context of a component in a Vector topology.
    component_type: Option<ComponentType>,

    /// Whether to disable the automatic derive for `serde::Serialize`.
    no_ser: bool,

    /// Whether to disable the automatic derive for `serde::Deserialize`.
    no_deser: bool,
}

impl FromMeta for Options {
    fn from_list(items: &[syn::NestedMeta]) -> darling::Result<Self> {
        let mut component_type = None;
        let mut no_ser = None;
        let mut no_deser = None;

        let mut errors = Error::accumulator();

        for nm in items {
            match nm {
                // Disable automatically deriving `serde::Serialize`.
                NestedMeta::Meta(Meta::Path(p)) if p == NO_SER => {
                    if no_ser.is_some() {
                        errors.push(Error::duplicate_field_path(p));
                    } else {
                        no_ser = Some(());
                    }
                }

                // Disable automatically deriving `serde::Deserialize`.
                NestedMeta::Meta(Meta::Path(p)) if p == NO_DESER => {
                    if no_deser.is_some() {
                        errors.push(Error::duplicate_field_path(p));
                    } else {
                        no_deser = Some(());
                    }
                }

                // Specified a component type.
                NestedMeta::Meta(Meta::List(ml))
                    if path_any(&ml.path, &[SOURCE, TRANSFORM, SINK]) =>
                {
                    if component_type.is_some() {
                        errors.push(Error::custom("component type already specified; `source`, `transform`, and `sink` are mutually exclusive").with_span(ml));
                    } else {
                        let maybe_component_name = ml.nested.first();
                        match maybe_component_name {
                            Some(NestedMeta::Lit(Lit::Str(component_name))) => {
                                component_type = Some(if ml.path == SOURCE {
                                    ComponentType::Source(component_name.value())
                                } else if ml.path == TRANSFORM {
                                    ComponentType::Transform(component_name.value())
                                } else if ml.path == SINK {
                                    ComponentType::Sink(component_name.value())
                                } else {
                                    unreachable!("asserted finite set of values in match arm guard")
                                });
                            }
                            _ => {
                                let path_nice = path_to_string(&ml.path);
                                let error = format!("`{}` must have only one parameter, the name of the component (i.e. `{}(\"name\")`)", path_nice, path_nice);
                                errors.push(Error::custom(&error).with_span(ml))
                            }
                        }
                    }
                }

                NestedMeta::Meta(m) => {
                    let error = "expected one of: `source(\"...\")`, `transform(\"...\")`, `sink(\"...\")`, `no_ser`, or `no_deser`";
                    errors.push(Error::custom(error).with_span(m));
                }

                NestedMeta::Lit(lit) => errors.push(Error::unexpected_lit_type(lit)),
            }
        }

        errors.finish().map(|()| Self {
            component_type,
            no_ser: no_ser.is_some(),
            no_deser: no_deser.is_some(),
        })
    }
}

impl Options {
    fn component_type(&self) -> Option<ComponentType> {
        self.component_type.clone()
    }

    fn should_derive_ser(&self) -> bool {
        !self.no_ser
    }

    fn should_derive_deser(&self) -> bool {
        !self.no_deser
    }
}

pub fn configurable_component_impl(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as AttributeArgs);
    let input = parse_macro_input!(item as DeriveInput);

    let options = match Options::from_list(&args) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(e.write_errors());
        }
    };

    let component_type = options.component_type().map(|ct| {
        let component_type = ct.as_type_str();
        let component_name = ct.as_name_str();

        quote! {
            #[configurable(metadata(component_type = #component_type))]
            #[::vector_config::component_name(#component_name)]
        }
    });

    let mut derives = Punctuated::<Path, Comma>::new();
    derives.push(parse_quote_spanned! {input.ident.span()=>
        ::vector_config_macros::Configurable
    });

    if options.should_derive_ser() {
        derives.push(parse_quote_spanned! {input.ident.span()=>
            ::serde::Serialize
        });
    }

    if options.should_derive_deser() {
        derives.push(parse_quote_spanned! {input.ident.span()=>
            ::serde::Deserialize
        });
    }

    let derived = quote! {
        #[derive(#derives)]
        #component_type
        #input
    };

    derived.into()
}
