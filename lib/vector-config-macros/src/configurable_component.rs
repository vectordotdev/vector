use darling::{util::path_to_string, Error, FromMeta};
use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, parse_quote, parse_quote_spanned, punctuated::Punctuated, token::Comma,
    AttributeArgs, DeriveInput, Ident, Lit, Meta, NestedMeta, Path,
};

#[derive(Copy, Clone)]
struct AttributeIdent(&'static str);

const ENRICHMENT_TABLE: AttributeIdent = AttributeIdent("enrichment_table");
const PROVIDER: AttributeIdent = AttributeIdent("provider");
const SINK: AttributeIdent = AttributeIdent("sink");
const SOURCE: AttributeIdent = AttributeIdent("source");
const TRANSFORM: AttributeIdent = AttributeIdent("transform");
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

fn path_matches(path: &Path, haystack: &[AttributeIdent]) -> bool {
    haystack.iter().any(|p| path == p)
}

#[derive(Clone, Debug)]
enum ComponentType {
    EnrichmentTable,
    Provider,
    Sink,
    Source,
    Transform,
}

impl<'a> From<&'a Path> for ComponentType {
    fn from(path: &'a Path) -> Self {
        let path_str = path_to_string(path);
        match path_str.as_str() {
            "enrichment_table" => Self::EnrichmentTable,
            "provider" => Self::Provider,
            "sink" => Self::Sink,
            "source" => Self::Source,
            "transform" => Self::Transform,
            _ => unreachable!("should not be used unless path is validated"),
        }
    }
}

#[derive(Clone, Debug)]
struct TypedComponent {
    component_type: ComponentType,
    component_name: Option<String>,
}

impl TypedComponent {
    /// Creates a new `TypedComponent`.
    const fn new(component_type: ComponentType) -> Self {
        Self {
            component_type,
            component_name: None,
        }
    }

    /// Creates a new `TypedComponent` with the given name.
    const fn with_name(component_type: ComponentType, component_name: String) -> Self {
        Self {
            component_type,
            component_name: Some(component_name),
        }
    }

    /// Gets the type of this component as a string.
    fn as_type_str(&self) -> &'static str {
        match self.component_type {
            ComponentType::EnrichmentTable => "enrichment_table",
            ComponentType::Provider => "provider",
            ComponentType::Sink => "sink",
            ComponentType::Source => "source",
            ComponentType::Transform => "transform",
        }
    }

    /// Creates the component description registration code based on the original derive input.
    ///
    /// If this typed component does not have a name, `None` will be returned, as only named
    /// components can be described.
    fn get_component_desc_registration(
        &self,
        input: &DeriveInput,
    ) -> Option<proc_macro2::TokenStream> {
        self.component_name.as_ref().map(|name| {
            let config_ty = &input.ident;
            let component_name = name.as_str();
            let desc_ty: syn::Type = match self.component_type {
                ComponentType::EnrichmentTable => {
                    parse_quote! { ::vector_config::component::EnrichmentTableDescription }
                }
                ComponentType::Provider => {
                    parse_quote! { ::vector_config::component::ProviderDescription }
                }
                ComponentType::Sink => parse_quote! { ::vector_config::component::SinkDescription },
                ComponentType::Source => {
                    parse_quote! { ::vector_config::component::SourceDescription }
                }
                ComponentType::Transform => {
                    parse_quote! { ::vector_config::component::TransformDescription }
                }
            };

            quote! {
                ::inventory::submit! {
                    #desc_ty::new::<#config_ty>(#component_name)
                }
            }
        })
    }

    /// Creates the component name registration code.
    ///
    /// If this typed component does not have a name, `None` will be returned, as only named
    /// components can be registered.
    fn get_component_name_registration(&self) -> Option<proc_macro2::TokenStream> {
        self.component_name.as_ref().map(|name| {
            let component_name = name.as_str();
            quote! {
                #[::vector_config::component_name(#component_name)]
            }
        })
    }
}

#[derive(Debug)]
struct Options {
    /// Component type details, if specified.
    ///
    /// While the macro `#[configurable_component]` sort of belies an implication that any item
    /// being annotated is a component, we only consider sources, transforms, and sinks a true
    /// "component", in the context of a component in a Vector topology.
    typed_component: Option<TypedComponent>,

    /// Whether to disable the automatic derive for `serde::Serialize`.
    no_ser: bool,

    /// Whether to disable the automatic derive for `serde::Deserialize`.
    no_deser: bool,
}

impl FromMeta for Options {
    fn from_list(items: &[syn::NestedMeta]) -> darling::Result<Self> {
        let mut typed_component = None;
        let mut no_ser = false;
        let mut no_deser = false;

        let mut errors = Error::accumulator();

        for nm in items {
            match nm {
                // Disable automatically deriving `serde::Serialize`.
                NestedMeta::Meta(Meta::Path(p)) if p == NO_SER => {
                    if no_ser {
                        errors.push(Error::duplicate_field_path(p));
                    } else {
                        no_ser = true;
                    }
                }

                // Disable automatically deriving `serde::Deserialize`.
                NestedMeta::Meta(Meta::Path(p)) if p == NO_DESER => {
                    if no_deser {
                        errors.push(Error::duplicate_field_path(p));
                    } else {
                        no_deser = true;
                    }
                }

                // Marked as a typed component that requires a name.
                NestedMeta::Meta(Meta::List(ml))
                    if path_matches(&ml.path, &[ENRICHMENT_TABLE, PROVIDER, SOURCE]) =>
                {
                    if typed_component.is_some() {
                        errors.push(
                            Error::custom("already marked as a typed component").with_span(ml),
                        );
                    } else {
                        match ml.nested.first() {
                            Some(NestedMeta::Lit(Lit::Str(component_name))) => {
                                typed_component = Some(TypedComponent::with_name(
                                    ComponentType::from(&ml.path),
                                    component_name.value(),
                                ));
                            }
                            _ => {
                                let path_nice = path_to_string(&ml.path);
                                let error = format!("`{}` must have only one parameter, the name of the component (i.e. `{}(\"name\")`)", path_nice, path_nice);
                                errors.push(Error::custom(&error).with_span(ml))
                            }
                        }
                    }
                }

                // Marked as a typed component that does not require a name.
                NestedMeta::Meta(Meta::Path(p)) if path_matches(p, &[SINK, TRANSFORM]) => {
                    if typed_component.is_some() {
                        errors.push(
                            Error::custom("already marked as a typed component").with_span(p),
                        );
                    } else {
                        typed_component = Some(TypedComponent::new(ComponentType::from(p)));
                    }
                }

                NestedMeta::Meta(m) => {
                    let error = "expected one of: `enrichment_table(\"...\")`, `provider(\"...\")`, `source(\"...\")`, `transform`, `sink`, `no_ser`, or `no_deser`";
                    errors.push(Error::custom(error).with_span(m));
                }

                NestedMeta::Lit(lit) => errors.push(Error::unexpected_lit_type(lit)),
            }
        }

        errors.finish().map(|()| Self {
            typed_component,
            no_ser,
            no_deser,
        })
    }
}

impl Options {
    fn typed_component(&self) -> Option<TypedComponent> {
        self.typed_component.clone()
    }

    fn skip_derive_ser(&self) -> bool {
        self.no_ser
    }

    fn skip_derive_deser(&self) -> bool {
        self.no_deser
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

    // If the component is typed (see `TypedComponent`/`ComponentType`), we do a few additional
    // things:
    // - we add a metadata attribute to indicate the component type
    // - we potentially add an attribute so the component's configuration type becomes "named",
    //   which drives the component config trait impl (i.e. `SourceConfig`) and will eventually
    //   drive the value that `serde` uses to deserialize the given component variant in the Big
    //   Enum model. this only happens if the component is actually named, and only sources are
    //   named at the moment.
    // - we automatically generate the call to register the component config type via `inventory`
    //   which powers the `vector generate` subcommand by maintaining a name -> config type map
    let component_type = options.typed_component().map(|tc| {
        let component_type = tc.as_type_str();
        let maybe_component_name_registration = tc.get_component_name_registration();

        quote! {
            #[configurable(metadata(component_type = #component_type))]
            #maybe_component_name_registration
        }
    });

    let maybe_component_desc = options
        .typed_component()
        .map(|tc| tc.get_component_desc_registration(&input));

    // Generate and apply all of the necessary derives.
    let mut derives = Punctuated::<Path, Comma>::new();
    derives.push(parse_quote_spanned! {input.ident.span()=>
        ::vector_config_macros::Configurable
    });

    if !options.skip_derive_ser() {
        derives.push(parse_quote_spanned! {input.ident.span()=>
            ::serde::Serialize
        });
    }

    if !options.skip_derive_deser() {
        derives.push(parse_quote_spanned! {input.ident.span()=>
            ::serde::Deserialize
        });
    }

    // Final assembly.
    let derived = quote! {
        #[derive(#derives)]
        #component_type
        #input
        #maybe_component_desc
    };

    derived.into()
}
