use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::{Sink, StreamExt};
use snafu::Snafu;
use std::error::Error;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::time::{self, Duration};
use vector_core::event::Event;
use vector_core::sink::StreamSink;
use vector_core::ByteSizeOf;

#[derive(Debug, Snafu)]
pub enum BuildError {}

#[derive(Debug, Snafu)]
pub enum LogApiError {}

#[derive(Debug)]
pub struct LogApiBuilder {}

impl LogApiBuilder {
    pub fn build(self) -> Result<LogApi, BuildError> {
        unimplemented!()
        // Ok(LogApi {})
    }
}

#[derive(Debug)]
pub struct LogApi {
    /// The total number of seconds before a flush is forced
    ///
    /// This value sets the total number of seconds that are allowed to ellapse
    /// prior to a flush of all buffered `Event` instances.
    timeout_seconds: u64,
    /// The total number of bytes this struct is allowed to hold
    ///
    /// This value acts as a soft limit on the amount of bytes this struct is
    /// allowed to hold prior to a flush happening. This limit is soft as if an
    /// event comes in and would cause `bytes_stored` to eclipse this value
    /// we'll need to temporarily store that event while a flush happens.
    bytes_stored_limit: usize,
    /// Tracks the total in-memory bytes being held by this struct
    ///
    /// This value tells us how many bytes our buffered `Event` instances are
    /// consuming. Once this value is >= `bytes_stored_limit` a flush will be
    /// triggered.
    bytes_stored: usize,
}

impl LogApi {
    pub fn new() -> LogApiBuilder {
        LogApiBuilder {}
    }
}

#[async_trait]
impl StreamSink for LogApi {
    async fn run(&mut self, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        // todo make this timeout the batch timeout
        let mut interval = time::interval(Duration::from_secs(self.timeout_seconds));
        tokio::select! {
            _ = interval.tick() => {
                // todo flush any accumulated events
                unimplemented!()
            },
            event = input.next() => {
                let event_size = event.size_of();
                // get the size

                // todo do a thing with th eevent
                unimplemented!()
            }
        }
    }
}
