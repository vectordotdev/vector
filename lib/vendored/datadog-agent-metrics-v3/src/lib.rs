//! V3 columnar protobuf codec for Datadog metrics.
//!
//! This crate implements the V3 columnar format for Datadog metrics payloads. Unlike the V2
//! row-based protobuf format where each metric is a complete message, V3 uses a columnar layout
//! with dictionary-based string deduplication for efficient encoding.
//!
//! [`V3Writer`] accumulates metrics one at a time via [`V3Writer::write`], then [`V3Writer::finalize`]
//! serializes the accumulated columns into a protobuf payload.

#![deny(missing_docs)]

mod constants;
mod interner;
mod types;
mod writer;

pub use constants::COLUMN_NAMES;
pub use types::V3MetricType;
pub use writer::{V3EncodeError, V3EncodedMetrics, V3EncoderStats, V3MetricBuilder, V3ValueEncodingStats, V3Writer};
