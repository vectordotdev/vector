#![deny(warnings)]

use proc_macro::TokenStream;

mod ast;
mod component_name;
mod configurable;
mod configurable_component;

/// Designates a type as being part of a Vector configuration.
///
/// This will automatically derive the [`Configurable`][vector_config::Configurable] trait for the given struct/enum, as
/// well as ensuring that serialization/deserialization (via `serde`) is derived.
///
/// ## Basics
///
/// In its most basic form, this attribute macro can be used to simply derive the aforementioned traits, making it using
/// in any other type also deriving `Configurable`:
///
/// ```no_run
/// use vector_config::configurable_component;
/// use serde;
///
/// /// Batching configurations.
/// #[configurable_component]
/// #[derive(Clone, Debug)]
/// pub struct BatchSettings {
///   // ...
/// }
/// ```
///
/// ## Component-specific modifiers
///
/// Additionally, callers can specify the component type, when being used directly on the top-level configuration object
/// for a component by specifying the component type (`source`, `transform`, or `sink`) as the sole parameter:
///
/// ```ignore
/// use vector_config::configurable_component;
/// use serde;
///
/// /// Configuration for the `kafka` source.
/// #[configurable_component(source("kafka"))]
/// #[derive(Clone, Debug)]
/// pub struct KafkaSourceConfig {
///   // ...
/// }
/// ```
///
/// This adds special metadata to the generated schema for that type indicating that it represents the configuration of
/// a component of the specified type.
///
/// ## Opting out of automatic derives
///
/// This macro will also derive the `Deserialize` and `Serialize` traits from `serde` automatically, as a way to clean
/// up the derives of a type which is already using `#[configurable_component]`. However, some types employ custom
/// (de)serialization implementations and do not need a standard derivation of those traits. In those cases, callers can
/// mark the type as not needing automatic derivations in a piecemeal fashion with the `no_ser` and/or `no_deser` modifiers:
///
/// ```no_run
/// use vector_config::configurable_component;
///
/// /// Helper type with custom deserialization logic.
/// #[configurable_component(no_deser)]
/// # #[derive(::serde::Deserialize)]
/// pub struct HelperTypeWithCustomDeser {
///   // This type brings its own implementation of `Deserialize` so we simply avoid bringing it in
///   // via `#[configurable_component]` but `Serialize` is still being automatically derived for us.
/// }
///
/// /// Helper type with entirely custom (de)serialization logic.
/// #[configurable_component(no_deser, no_ser)]
/// # #[derive(::serde::Deserialize, ::serde::Serialize)]
/// pub struct HelperTypeWithCustomDeserAndSer {
///   // This type brings its own implementation of `Deserialize` _and_ `Serialize` so we've avoided
///   // having them automatically derived via `#[configurable_component]`.
/// }
/// ```
#[proc_macro_attribute]
pub fn configurable_component(attrs: TokenStream, item: TokenStream) -> TokenStream {
    configurable_component::configurable_component_impl(attrs, item)
}

/// Generates an implementation of the `Configurable` trait for the given container.
///
/// In general, `#[configurable_component]` should be preferred as it ensures the other necessary derives/trait
/// implementations are provided, and offers other features related to describing specific configuration types, etc.
#[proc_macro_derive(Configurable, attributes(configurable))]
pub fn derive_configurable(input: TokenStream) -> TokenStream {
    configurable::derive_configurable_impl(input)
}

/// Generates an implementation of the `NamedComponent` trait for the given container.
///
/// The only valid form of this macro is `#[component_name("something")]`.
#[proc_macro_attribute]
pub fn component_name(attrs: TokenStream, item: TokenStream) -> TokenStream {
    component_name::component_name_impl(attrs, item)
}
