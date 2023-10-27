use darling::util::path_to_string;
use proc_macro::TokenStream;

use quote::quote;
use syn::{parse_macro_input, spanned::Spanned, Attribute, DeriveInput, Error, LitStr};
use vector_config_common::configurable_package_name_hack;

use crate::attrs::{self, path_matches};

pub fn derive_component_name_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let ident = &input.ident;

    // This derive macro has to do a bunch of heavy lifting here, but it mainly boils down to two
    // things: validating that a name is given for the component (and only one name), and spitting
    // out a component type-specific error message otherwise.
    //
    // Firstly, we want to give contextual error messages to users whenever possible, so that means
    // that if a user is transitively deriving this macro because they're using
    // `#[configurable_component(source)]`, we want to let them know that sources must have a name
    // defined.
    //
    // It's easier to do that in the `configurable_component` macro, but errors in attribute macros
    // lead to the item being annotated essentially being excluded from the compiled output... so
    // you see the error that the attribute macro emits... and then you see an error anywhere else
    // some code is trying to reference that struct or enum or what have you. This leads to a large
    // number of errors that are technically related to the problem at hand, but ultimately are all
    // solved by fixing the error with the macro usage.
    //
    // Derive macros function differently, such that the original item is always left alone and only
    // new tokens are generated, so we can emit errors without causing more errors, but we've lost
    // some of the semantic information about the original usage of `configurable_component` by the
    // time we're here, running _this_ macro.
    //
    // To deal with this, we specifically look for component type-specific attributes that define
    // the name, and higher up in `configurable_component`, we use the appropriate attribute for the
    // component type at hand. This ends up giving output that follows this pattern:
    //
    // `#[configurable_component(source)]` -> `#[source_component]`
    // `#[configurable_component(source("foo"))]` -> `#[source_component("foo")]`
    //
    // This allows us to determine the component type originally passed to `configurable_component`
    // so that, in the case of the nameless example above, we can generate an error message attached
    // to the span for `source`, such as: "sources must specify a name (e.g. `source("name")`)"
    //
    // Secondly, and finally, we always expect a single one of these helper attributes defining the
    // name: no more, no less. Even though `configurable_component` should correctly adhere to that,
    // we still need to go through the motions of verifying it here... which may capture either
    // someone manually using the derive incorrectly, or an actual bug in our usage via other macros.
    let mut errors = Vec::new();
    let mut component_names = input
        .attrs
        .iter()
        .filter_map(|attr| match attr_to_component_name(attr) {
            Ok(component_name) => component_name,
            Err(e) => {
                errors.push(e);
                None
            }
        })
        .collect::<Vec<_>>();

    if !errors.is_empty() {
        let mut main_error = errors.remove(0);
        for error in errors.drain(..) {
            main_error.combine(error);
        }

        return main_error.into_compile_error().into();
    }

    // Any component names we have now have been validated, so we just need to check and make sure
    // we actually have one, and only one, and spit out the correct errors otherwise.
    if component_names.is_empty() {
        return Error::new(
            ident.span(),
            "component must have a name defined (e.g. `#[component_name(\"foobar\")]`)",
        )
        .into_compile_error()
        .into();
    }

    if component_names.len() > 1 {
        return Error::new(ident.span(), "component cannot have multiple names defined")
            .into_compile_error()
            .into();
    }

    let component_name = component_names.remove(0);

    // We have a single, valid component name, so let's actually spit out our derive.
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let vector_config = configurable_package_name_hack();
    let derived = quote! {
        impl #impl_generics #ident #ty_generics #where_clause {
            pub(super) const NAME: &'static str = #component_name;
        }

        impl #impl_generics #vector_config::NamedComponent for #ident #ty_generics #where_clause {
            fn get_component_name(&self) -> &'static str {
                #component_name
            }
        }
    };
    derived.into()
}

fn attr_to_component_name(attr: &Attribute) -> Result<Option<String>, Error> {
    // First, filter out anything that isn't ours.
    if !path_matches(
        attr.path(),
        &[
            attrs::ENRICHMENT_TABLE_COMPONENT,
            attrs::PROVIDER_COMPONENT,
            attrs::SINK_COMPONENT,
            attrs::SOURCE_COMPONENT,
            attrs::TRANSFORM_COMPONENT,
            attrs::SECRETS_COMPONENT,
        ],
    ) {
        return Ok(None);
    }

    // Reconstruct the original attribute path (i.e. `source`) from our marker version of it (i.e.
    // `source_component`), so that any error message we emit is contextually relevant.
    let path_str = path_to_string(attr.path());
    let component_type_attr = path_str.replace("_component", "");
    let component_type = component_type_attr.replace('_', " ");

    // Make sure the attribute actually has inner tokens. If it doesn't, this means they forgot
    // entirely to specify a component name, and we want to give back a meaningful error that looks
    // correct when applied in the context of `#[configurable_component(...)]`.
    if attr.meta.require_list().is_err() {
        return Err(Error::new(
            attr.span(),
            format!(
                "{}s must have a name specified (e.g. `{}(\"my_component\")`)",
                component_type, component_type_attr
            ),
        ));
    }

    // Now try and parse the helper attribute as a literal string, which is the only valid form.
    // After that, make sure it's actually valid according to our naming rules.
    attr.parse_args::<LitStr>()
        .map_err(|_| {
            Error::new(
                attr.span(),
                format!(
                    "expected a string literal for the {} name (i.e. `{}(\"...\")`)",
                    component_type, component_type_attr
                ),
            )
        })
        .and_then(|component_name| {
            let component_name_str = component_name.value();
            check_component_name_validity(&component_name_str)
                .map_err(|e| Error::new(component_name.span(), e))
                .map(|()| Some(component_name_str))
        })
}

fn check_component_name_validity(component_name: &str) -> Result<(), String> {
    // In a nutshell, component names must contain only lowercase ASCII alphabetic characters, or
    // numbers, or underscores.

    if component_name.is_empty() {
        return Err("component name must be non-empty".to_string());
    }

    // We only support ASCII names, so get that out of the way.
    if !component_name.is_ascii() {
        return Err("component names may only contain ASCII characters".to_string());
    }

    // Now, we blindly try and convert the given component name into the correct format, and
    // if the result doesn't match the input, then we know the input is invalid... but then we also
    // have an example string to show in the error to explain what the user needs to specify.
    let component_name_converted = component_name
        .chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| if !c.is_ascii_alphanumeric() { '_' } else { c })
        .collect::<String>();

    if component_name == component_name_converted {
        Ok(())
    } else {
        Err(format!("component names must be lowercase, and contain only letters, numbers, and underscores (e.g. \"{}\")", component_name_converted))
    }
}
