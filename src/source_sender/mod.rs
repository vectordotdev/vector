#![allow(missing_docs)]

mod builder;
mod errors;
mod output;
mod sender;
#[cfg(test)]
mod tests;

pub use builder::Builder;
pub use errors::{ClosedError, StreamSendError};
use output::Output;
pub use sender::{SourceSender, SourceSenderItem};

pub(crate) const CHUNK_SIZE: usize = 1000;

#[cfg(any(test, feature = "test-utils"))]
const TEST_BUFFER_SIZE: usize = 100;

const LAG_TIME_NAME: &str = "source_lag_time_seconds";
