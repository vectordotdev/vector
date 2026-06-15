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

use std::sync::atomic::{AtomicUsize, Ordering};

/// Default number of events batched per source send, and the base used for source output buffer
/// sizing. Used when the chunk size has not been configured at startup.
pub const DEFAULT_CHUNK_SIZE: usize = 1000;

static CHUNK_SIZE: AtomicUsize = AtomicUsize::new(0);

/// Returns the configured source sender chunk size, or [`DEFAULT_CHUNK_SIZE`] if unset.
#[must_use]
pub fn chunk_size() -> usize {
    match CHUNK_SIZE.load(Ordering::Relaxed) {
        0 => DEFAULT_CHUNK_SIZE,
        size => size,
    }
}

/// Sets the process-wide source sender chunk size. Must be called at most once, before the
/// topology is built. Panics if called more than once.
pub fn set_chunk_size(size: usize) {
    CHUNK_SIZE
        .compare_exchange(0, size, Ordering::Acquire, Ordering::Relaxed)
        .unwrap_or_else(|_| panic!("double chunk_size initialization"));
}

#[cfg(any(test, feature = "test"))]
const TEST_BUFFER_SIZE: usize = 100;

use vector_common::internal_event::HistogramName;

const LAG_TIME_NAME: HistogramName = HistogramName::SourceLagTimeSeconds;
const SEND_LATENCY_NAME: HistogramName = HistogramName::SourceSendLatencySeconds;
const SEND_BATCH_LATENCY_NAME: HistogramName = HistogramName::SourceSendBatchLatencySeconds;
