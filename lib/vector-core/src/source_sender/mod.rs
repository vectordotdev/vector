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
/// Implement this trait to mutate events just before they are placed on the output channel.
/// Because each method receives a typed reference, it is impossible at the type level to
/// accidentally change an event's variant.
///
/// It is applied *globally* — to all outputs (default and named ports) produced by the same
/// [`Builder`].
pub trait PostProcessor: Send + Sync {
    /// Called once for every [`crate::event::LogEvent`] in a batch.
    fn process_log(&self, event: &mut crate::event::LogEvent);
    /// Called once for every [`crate::event::Metric`] in a batch.
    fn process_metric(&self, event: &mut crate::event::Metric);
    /// Called once for every [`crate::event::TraceEvent`] in a batch.
    fn process_trace(&self, event: &mut crate::event::TraceEvent);
}
