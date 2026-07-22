//! Obstack on-the-wire contract for the `iggy` sink.
//!
//! This used to be a hand-vendored, byte-compatible copy of Obstack's label/row
//! model, shard placement, and queue-envelope codec. It is now a thin re-export
//! of the canonical [`obstack_wire`] crate, which both this producer and the
//! Obstack consumer depend on, so the wire format can no longer drift between
//! the two repositories. The rest of the sink keeps importing from
//! `super::proto::*`, so those call sites are unchanged.
//!
//! `encode_ref` and `shard_of_fingerprint` are only exercised by the sink's
//! `#[cfg(test)]` module, so they read as unused in a normal build — the
//! re-export intentionally preserves the full producer surface regardless.
#![allow(unused_imports)]

pub use obstack_wire::{
    DEFAULT_TOPIC_PREFIX, Label, Labels, LogRow, MetricExemplar, PRODUCER_PARTITIONS,
    ProducerIdentity, ProducerRegistration, QueueGeneration, SampleRow, ScalarValue, SpanEvent,
    SpanKind, SpanLink, SpanRow, StatusCode, WriteBatch, decode_registration, encode_chunks,
    encode_ref, encode_registration, registration_message_id, shard_of_fingerprint,
    stable_message_id,
};
