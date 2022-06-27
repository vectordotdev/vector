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
/// ### Examples
///
/// In its most basic form, this attribute macro can be used to simply derive the aforementioned traits, making it using
/// in any other type also deriving `Configurable`:
///
/// ```no_run
/// use vector_config_macros::configurable_component;
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
/// Additionally, callers can specify the component type, when being used directly on the top-level configuration object
/// for a component by specifying the component type (`source`, `transform`, or `sink`) as the sole parameter:
///
/// ```no_run
/// use vector_config_macros::configurable_component;
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
