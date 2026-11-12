//! Decoders used only by tests of wire compatibility with older Vector receivers.
//!
//! A separate crate gives Prost 0.13's generated code its own `prost` dependency,
//! including its derive macros, independently of the production Prost version.

use prost::Message as _;

#[allow(dead_code, deprecated)] // The compatibility tests need only the top-level decoders.
#[allow(
    clippy::doc_markdown,
    clippy::struct_field_names,
    clippy::trivially_copy_pass_by_ref
)] // Preserve the code emitted by the previous Prost generator.
mod event {
    include!(concat!(env!("OUT_DIR"), "/event.rs"));
}

/// Tests whether a receiver using Prost 0.13 can decode an event array.
#[must_use]
pub fn event_array_decodes(bytes: &[u8]) -> bool {
    event::EventArray::decode(bytes).is_ok()
}

/// Tests whether a receiver using Prost 0.13 can decode a legacy event wrapper.
#[must_use]
pub fn event_wrapper_decodes(bytes: &[u8]) -> bool {
    event::EventWrapper::decode(bytes).is_ok()
}
