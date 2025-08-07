//! `redis` sink.
//!
//! Writes data to [redis](https://redis.io/).
mod config;
mod request_builder;
mod service;
mod sink;

#[cfg(test)]
mod tests;

#[cfg(feature = "redis-integration-tests")]
#[cfg(test)]
mod integration_tests;

use bytes::Bytes;
use redis::RedisError;
use snafu::Snafu;
use tokio::sync::watch::error::RecvError;

use crate::sinks::prelude::*;

use self::{
    config::{ListMethod, SortedSetMethod},
    sink::GenerationCount,
};

use super::util::EncodedLength;

#[derive(Debug, Snafu)]
pub(super) enum RedisSinkError {
    #[snafu(display("Creating Redis producer failed: {source}"))]
    RedisCreateFailed { source: RedisError },
    #[snafu(display(
        "Error sending query: {source}{}",
        if let Some(gen) = generation { format!(", gen={gen}") } else { String::new() }
    ))]
    SendError {
        source: RedisError,
        generation: Option<GenerationCount>,
    },
    #[snafu(display("Repair channel was closed: {source}"))]
    RepairChannelError { source: RecvError },
}

#[derive(Clone, Copy, Debug, Derivative)]
#[derivative(Default)]
pub enum DataType {
    /// The Redis `list` type.
    ///
    /// This resembles a deque, where messages can be popped and pushed from either end.
    #[derivative(Default)]
    List(ListMethod),

    /// The Redis `sorted set` type.
    ///
    /// This resembles a priority queue, where messages can be pushed with a score.
    SortedSet(SortedSetMethod),

    /// The Redis `channel` type.
    ///
    /// Redis channels function in a pub/sub fashion, allowing many-to-many broadcasting and receiving.
    Channel,
}

/// Wrapper for an `Event` that also stored the rendered key.
pub(super) struct RedisEvent {
    event: Event,
    key: String,
    score: Option<u64>,
}

impl Finalizable for RedisEvent {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.event.take_finalizers()
    }
}

impl ByteSizeOf for RedisEvent {
    fn allocated_bytes(&self) -> usize {
        self.event.allocated_bytes()
    }
}

impl GetEventCountTags for RedisEvent {
    fn get_tags(&self) -> TaggedEventsSent {
        self.event.get_tags()
    }
}

impl EstimatedJsonEncodedSizeOf for RedisEvent {
    fn estimated_json_encoded_size_of(&self) -> JsonSize {
        self.event.estimated_json_encoded_size_of()
    }
}

#[derive(Clone)]
pub(super) struct RedisRequest {
    request: Vec<RedisKvEntry>,
    finalizers: EventFinalizers,
    metadata: RequestMetadata,
}

impl Finalizable for RedisRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for RedisRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

#[derive(Debug, Clone)]
pub(super) struct RedisKvEntry {
    key: String,
    value: Bytes,
    score: Option<u64>,
}

impl EncodedLength for RedisKvEntry {
    fn encoded_length(&self) -> usize {
        self.value.len()
    }
}
