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

const LAG_TIME_NAME: &str = "source_lag_time_seconds";
const SEND_LATENCY_NAME: &str = "source_send_latency_seconds";
const SEND_BATCH_LATENCY_NAME: &str = "source_send_batch_latency_seconds";
