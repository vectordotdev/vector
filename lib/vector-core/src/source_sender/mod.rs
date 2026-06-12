#![allow(
    missing_docs,
    clippy::missing_errors_doc,
    clippy::doc_markdown,
    clippy::missing_panics_doc
)]

mod builder;
mod errors;
mod output;
mod sender;
#[cfg(test)]
mod tests;

pub use builder::Builder;
pub use errors::SendError;
use output::{Output, OutputMetrics};
pub use sender::{SourceSender, SourceSenderItem};

pub const CHUNK_SIZE: usize = 1000;

#[cfg(any(test, feature = "test"))]
const TEST_BUFFER_SIZE: usize = 100;

use vector_common::internal_event::HistogramName;

const LAG_TIME_NAME: HistogramName = HistogramName::SourceLagTimeSeconds;
const SEND_LATENCY_NAME: HistogramName = HistogramName::SourceSendLatencySeconds;
const SEND_BATCH_LATENCY_NAME: HistogramName = HistogramName::SourceSendBatchLatencySeconds;

/// A post-processing step applied to every event that flows through a [`SourceSender`].
///
/// The hook executes on each event immediately before the event is placed on the output channel.
/// Metadata (schema definition, upstream ID) is attached to the event **after** the closure runs,
/// so the closure's mutations are always preserved even if it replaces the entire event value.
///
/// It is applied *globally* — to all outputs (default and named ports) produced by the same
/// [`Builder`].
///
/// # Variant-preservation contract
///
/// The closure **MUST NOT change an event's variant**: a log must remain a log, a metric must
/// remain a metric, and a trace must remain a trace. Violating this contract causes a panic in
/// debug builds (enforced via `debug_assert_eq!` on [`std::mem::discriminant`]).
///
/// Currently one variant is provided:
///
/// - [`PostProcessor::HardCoded`]: calls an infallible Rust closure. No events are dropped by
///   this variant.
///
/// If per-output post-processing is needed in the future, a `with_post_processor_for_port` API
/// can be added without breaking this interface.
#[derive(Clone)]
pub enum PostProcessor {
    /// Call a hard-coded Rust function against every event.
    ///
    /// The closure is infallible; no events are dropped by this variant.
    ///
    /// # Contract
    ///
    /// The closure **must not change the event's variant** (log → log, metric → metric, trace →
    /// trace). Changing the variant violates the variant-preservation contract and will panic in
    /// debug builds.
    HardCoded(std::sync::Arc<dyn Fn(&mut crate::event::Event) + Send + Sync>),
}
