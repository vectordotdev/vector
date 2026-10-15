//! Streaming wire-format to Arrow encoder.
//!
//! Parses proto wire bytes in a single pass and appends values directly into
//! Arrow `RecordBatch` column builders, skipping the `DynamicMessage` /
//! `LogEvent` intermediate representations used by the generic
//! `ProtobufDeserializer` + `ArrowStreamSerializer` path.
//!
//! Used as a [`BatchSerializerConfig`] variant — the upstream source/transform
//! is expected to place the original proto wire bytes in the event's `message`
//! field (Vector convention).
//!
//! Failure semantics are split:
//!   * Event-shape problems (missing `message` field, non-`Bytes` value) fail
//!     the batch — the pipeline is misconfigured if any event reaches here in
//!     the wrong shape.
//!   * Wire-format decode errors are isolated to the offending row: the row
//!     is dropped from the output `RecordBatch`, counted via the
//!     `wire_to_arrow_rows_dropped` metric, and a sample error is logged.
//!     One poison message can't poison the whole batch.
//!
//! ## Scope
//!
//! The encoder takes one `MessageDescriptor` and decodes the bytes in
//! `event.message` against it, emitting one `RecordBatch` row per event. It
//! is agnostic to how the caller produced those bytes and to what any
//! particular schema represents. If the incoming payload requires any
//! pre-processing — multi-frame unwrapping, decompression, merging bytes from
//! multiple sources, sink-time / build-time stamps — perform it upstream (in
//! VRL or a custom transform) so that `event.message` holds a single
//! self-contained byte stream that matches the configured descriptor.
//!
//! ## Supported today
//!
//! - Scalar proto fields (int32/int64/uint32/uint64/sint32/sint64/fixed*/float/double/bool/string/bytes/enum)
//! - Singular nested messages -> Arrow `Struct`
//! - Repeated nested messages -> Arrow `List<Struct>`
//! - Repeated scalars (packed and unpacked) -> Arrow `List<primitive>`
//! - Proto maps (`map<K, V>`) -> Arrow `Map<Struct(key, value)>`
//! - Oneof variants
//! - `int64 -> Timestamp(Microsecond, tz)` coercion
//!
//! [`BatchSerializerConfig`]: crate::encoding::BatchSerializerConfig

mod append;
mod builders;
mod encoder;
mod errors;
mod plan;
mod scan;
mod serializer;
mod wire;

#[cfg(test)]
mod tests;

pub use encoder::WireToArrowEncoder;
pub use errors::WireToArrowError;
pub use serializer::{WireToArrowSerializer, WireToArrowSerializerConfig};
