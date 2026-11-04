#![deny(warnings)]

use proc_macro::TokenStream;

mod internal_event;

/// Derives `NamedInternalEvent` so `InternalEvent::name()` returns a stable
/// compile-time identifier for the event type.
///
/// Apply this derive to any struct that also implements `InternalEvent` or `RegisterInternalEvent`:
///
/// ```ignore
/// use vector_lib::internal_event::{InternalEvent, NamedInternalEvent};
///
/// #[derive(Debug, NamedInternalEvent)]
/// pub struct UdpSendIncompleteError {
///     pub data_size: usize,
///     pub sent: usize,
/// }
///
/// impl InternalEvent for UdpSendIncompleteError {
///     fn emit(self) {
///         // ... emit metrics/logging ...
///     }
/// }
///
/// // Later, `UdpSendIncompleteError::name()` returns the string "UdpSendIncompleteError".
/// ```
///
/// Notes:
/// - Works with lifetimes and generics.
/// - The generated implementation returns `stringify!(TypeName)` which avoids
///   compiler-version-dependent module paths.
#[proc_macro_derive(NamedInternalEvent)]
pub fn derive_internal_event_name(input: TokenStream) -> TokenStream {
    internal_event::derive_impl_named_internal_event(input)
}
