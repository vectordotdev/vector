use darling::{ast::NestedMeta, Error, FromMeta};
use proc_macro::TokenStream;
use proc_macro2::{Ident, Span};
use quote::{quote, quote_spanned};
use syn::{
    parse_macro_input, parse_quote, parse_quote_spanned, punctuated::Punctuated, spanned::Spanned,
    token::Comma, DeriveInput, Lit, LitStr, Meta, MetaList, Path,
};
use vector_config_common::{
    constants::ComponentType, human_friendly::generate_human_friendly_string,
};

use crate::attrs;

#[derive(Clone, Debug)]
struct TypedComponent {
    span: Span,
    component_type: ComponentType,
    component_name: Option<LitStr>,
    description: Option<LitStr>,
}

impl TypedComponent {
    /// Creates a new `TypedComponent` from the given path.
    ///
    /// If the path does not matches a known component type, `None` is returned. Otherwise,
    /// `Some(...)` is returned with a valid `TypedComponent`.
    fn from_path(path: &Path) -> Option<Self> {
        ComponentType::try_from(path)
            .ok()
            .map(|component_type| Self {
                span: path.span(),
                component_type,
                component_name: None,
                description: None,
            })
    }

    /// Creates a new `TypedComponent` from the given meta list.
    ///
    /// If the meta list does not have a path that matches a known component type, `None` is
    /// returned. Otherwise, `Some(...)` is returned with a valid `TypedComponent`.
    fn from_meta_list(ml: &MetaList) -> Option<Self> {
        let mut items = ml
            .parse_args_with(Punctuated::<NestedMeta, Comma>::parse_terminated)
            .unwrap_or_default()
            .into_iter();
        ComponentType::try_from(&ml.path)
            .ok()
            .map(|component_type| {
                let component_name = match items.next() {
                    Some(NestedMeta::Lit(Lit::Str(component_name))) => Some(component_name),
                    _ => None,
                };
                let description = match items.next() {
                    Some(NestedMeta::Lit(Lit::Str(description))) => Some(description),
                    _ => None,
                };
                Self {
                    span: ml.span(),
                    component_type,
                    component_name,
                    description,
                }
            })
    }

    /// Gets the component name, if one was specified.
    fn get_component_name(&self) -> Option<String> {
        self.component_name.as_ref().map(|s| s.value())
    }

    /// Creates the component description registration code based on the original derive input.
    ///
    /// If this typed component does not have a name, `None` will be returned, as only named
    /// components can be described.
    fn get_component_desc_registration(
        &self,
        input: &DeriveInput,
    ) -> Option<proc_macro2::TokenStream> {
        self.component_name.as_ref().map(|component_name| {
            let config_ty = &input.ident;
            let desc_ty: syn::Type = match self.component_type {
                ComponentType::Api => {
                    parse_quote! { ::vector_config::component::ApiDescription }
                }
                ComponentType::EnrichmentTable => {
                    parse_quote! { ::vector_config::component::EnrichmentTableDescription }
                }
                ComponentType::GlobalOption => {
                    parse_quote! { ::vector_config::component::GlobalOptionDescription }
                }
                ComponentType::Provider => {
                    parse_quote! { ::vector_config::component::ProviderDescription }
                }
                ComponentType::Secrets => {
                    parse_quote! { ::vector_config::component::SecretsDescription }
                }
                ComponentType::Sink => parse_quote! { ::vector_config::component::SinkDescription },
                ComponentType::Source => {
                    parse_quote! { ::vector_config::component::SourceDescription }
                }
                ComponentType::Transform => {
                    parse_quote! { ::vector_config::component::TransformDescription }
                }
            };

            // Derive the human-friendly name from the component name.
            let label = generate_human_friendly_string(&component_name.value());

            // Derive the logical name from the config type, with the trailing "Config" dropped.
            let logical_name = config_ty.to_string();
            let logical_name = logical_name.strip_suffix("Config").unwrap_or(&logical_name);

            // TODO: Make this an `expect` once all component types have been converted.
            let description = self
                .description
                .as_ref()
                .map(LitStr::value)
                .unwrap_or_else(|| "This component is missing a description.".into());

            quote! {
                ::inventory::submit! {
                    #desc_ty::new::<#config_ty>(
                        #component_name,
                        #label,
                        #logical_name,
                        #description,
                    )
                }
            }
        })
    }

    /// Creates the component name registration code.
    fn get_component_name_registration(&self) -> proc_macro2::TokenStream {
        let helper_attr = get_named_component_helper_ident(self.component_type);
        match self.component_name.as_ref() {
            None => quote_spanned! {self.span=>
                #[derive(::vector_config::NamedComponent)]
                #[#helper_attr]
            },
            Some(component_name) => quote_spanned! {self.span=>
                #[derive(::vector_config::NamedComponent)]
                #[#helper_attr(#component_name)]
            },
        }
    }
}

#[derive(Debug)]
struct Options {
    /// Component type details, if specified.
    ///
    /// While the macro `#[configurable_component]` sort of belies an implication that any item
    /// being annotated is a component, we make a distinction here in terms of what can be a
    /// component in a Vector topology, versus simply what is allowed as a configurable "component"
    /// within a Vector configuration.
    typed_component: Option<TypedComponent>,

    /// Whether to disable the automatic derive for `serde::Serialize`.
    no_ser: bool,

    /// Whether to disable the automatic derive for `serde::Deserialize`.
    no_deser: bool,
}

impl FromMeta for Options {
    fn from_list(items: &[NestedMeta]) -> darling::Result<Self> {
        let mut typed_component = None;
        let mut no_ser = false;
        let mut no_deser = false;

        let mut errors = Error::accumulator();

        for nm in items {
            match nm {
                // Disable automatically deriving `serde::Serialize`.
                NestedMeta::Meta(Meta::Path(p)) if p == attrs::NO_SER => {
                    if no_ser {
                        errors.push(Error::duplicate_field_path(p));
                    } else {
                        no_ser = true;
                    }
                }

                // Disable automatically deriving `serde::Deserialize`.
                NestedMeta::Meta(Meta::Path(p)) if p == attrs::NO_DESER => {
                    if no_deser {
                        errors.push(Error::duplicate_field_path(p));
                    } else {
                        no_deser = true;
                    }
                }

                // Marked as a typed component that requires a name.
                NestedMeta::Meta(Meta::List(ml)) if ComponentType::is_valid_type(&ml.path) => {
                    if typed_component.is_some() {
                        errors.push(
                            Error::custom("already marked as a typed component").with_span(ml),
                        );
                    } else {
                        let result = TypedComponent::from_meta_list(ml);
                        if result.is_none() {
                            return Err(Error::custom("meta list matched named component type, but failed to parse into TypedComponent").with_span(&ml));
                        }

                        typed_component = result;
                    }
                }

                // Marked as a typed component that requires a name, but it was not specified.
                //
                // When marked as a typed component, but no name is specified, we still want to
                // generate our normal derive output, as we let the `NamedComponent` derive handle
                // emitting an error to tell the user that the component type requires a name,
                //
                // We don't emit those errors here because errors in attribute macros will cause a
                // cascading set of errors that are too noisy.
                NestedMeta::Meta(Meta::Path(p)) if ComponentType::is_valid_type(p) => {
                    if typed_component.is_some() {
                        errors.push(
                            Error::custom("already marked as a typed component").with_span(p),
                        );
                    } else {
                        let result = TypedComponent::from_path(p);
                        if result.is_none() {
                            return Err(Error::custom("path matched component type, but failed to parse into TypedComponent").with_span(p));
                        }

                        typed_component = result;
                    }
                }

                NestedMeta::Meta(m) => {
                    let error = "expected one of: `enrichment_table(\"...\")`, `provider(\"...\")`, `source(\"...\")`, `transform(\"...\")`, `secrets(\"...\")`, `sink(\"...\")`, `no_ser`, or `no_deser`";
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
    let args: Vec<NestedMeta> =
        parse_macro_input!(args with Punctuated::<NestedMeta, Comma>::parse_terminated)
            .into_iter()
            .collect();
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
        let component_type = tc.component_type.as_str();
        quote! {
            #[configurable(metadata(docs::component_type = #component_type))]
        }
    });

    let maybe_component_name = options.typed_component().map(|tc| {
        let maybe_component_name_registration = tc.get_component_name_registration();
        let maybe_component_name_metadata = tc
            .get_component_name()
            .map(|name| quote! { #[configurable(metadata(docs::component_name = #name))] });

        quote! {
            #maybe_component_name_metadata
            #maybe_component_name_registration
        }
    });

    let maybe_component_desc = options
        .typed_component()
        .map(|tc| tc.get_component_desc_registration(&input));

    // Generate and apply all of the necessary derives.
    let mut derives = Punctuated::<Path, Comma>::new();
    derives.push(parse_quote_spanned! {input.ident.span()=>
        ::vector_config::Configurable
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
        #maybe_component_name
        #input
        #maybe_component_desc
    };

    derived.into()
}

/// Gets the ident of the component type-specific helper attribute for the `NamedComponent` derive.
///
/// When we emit code for a configurable item that has been marked as a typed component, we
/// optionally emit the code to generate an implementation of `NamedComponent` if that component
/// is supposed to be named.
///
/// This function returns the appropriate ident for the helper attribute specific to the
/// component, as we must pass the component type being named -- source vs transform, etc --
/// down to the derive for `NamedComponent`. This allows it to emit error messages that _look_
/// like they're coming from `configurable_component`, even though they're coming from the
/// derive for `NamedComponent`.
fn get_named_component_helper_ident(component_type: ComponentType) -> Ident {
    let attr = match component_type {
        ComponentType::Api => attrs::API_COMPONENT,
        ComponentType::EnrichmentTable => attrs::ENRICHMENT_TABLE_COMPONENT,
        ComponentType::GlobalOption => attrs::GLOBAL_OPTION_COMPONENT,
        ComponentType::Provider => attrs::PROVIDER_COMPONENT,
        ComponentType::Secrets => attrs::SECRETS_COMPONENT,
        ComponentType::Sink => attrs::SINK_COMPONENT,
        ComponentType::Source => attrs::SOURCE_COMPONENT,
        ComponentType::Transform => attrs::TRANSFORM_COMPONENT,
    };

    attr.as_ident(Span::call_site())
}
