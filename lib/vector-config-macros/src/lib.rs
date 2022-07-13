#![deny(warnings)]

use proc_macro::TokenStream;

mod ast;
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
/// /// Configuration for the Something struct
/// #[configurable_component]
/// #[derive(Clone, Debug)]
/// pub struct Something {
///   // ...
/// }
/// ```
///
/// ## Component-specific modifiers
///
/// Additionally, callers can specify the component type, when being used directly on the top-level configuration object
/// for a component by specifying the component type (`source`, `transform`, or `sink`) as the sole parameter:
///
/// ```no_run
/// use vector_config::configurable_component;
/// use serde;
///
/// /// Configuration for the `kafka` source
/// #[configurable_component(source)]
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
/// ```norun
/// use vector_config::configurable_component;
///
/// // This keeps the automatic derive for `Serialize` but doesn't bother deriving `Deserialize`:
/// #[configurable_component(no_deser)]
/// pub struct HelperTypeWithCustomDeser {
///   // ...
/// }
///
/// // If we don't require either, we can disable them both:
/// #[configurable_component(no_deser, no_ser)]
/// pub struct HelperTypeWithCustomDeserAndSer {
///   // ...
/// }
/// ```
#[proc_macro_attribute]
pub fn configurable_component(args: TokenStream, item: TokenStream) -> TokenStream {
    configurable_component::configurable_component_impl(args, item)
}

/// Generates an implementation of `Configurable` trait for the given container.
///
/// In general, `#[configurable_component]` should be preferred as it ensures the other necessary derives/trait
/// implementations are provided, and offers other features related to describing specific configuration types, etc.
#[proc_macro_derive(Configurable, attributes(configurable))]
pub fn derive_configurable(input: TokenStream) -> TokenStream {
    configurable::derive_configurable_impl(input)
}
