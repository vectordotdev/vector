pub mod adaptive_concurrency;
pub mod auth;
// https://github.com/mcarton/rust-derivative/issues/112
#[allow(clippy::non_canonical_clone_impl)]
pub mod batch;
pub mod buffer;
pub mod builder;
pub mod compressor;
pub mod encoding;
pub mod http;
pub mod metadata;
pub mod normalizer;
pub mod partitioner;
pub mod processed_event;
pub mod request_builder;
pub mod retries;
pub mod service;
pub mod sink;
pub mod snappy;
pub mod socket_bytes_sink;
pub mod statistic;
pub mod tcp;
#[cfg(test)]
pub mod test;
pub mod udp;
#[cfg(all(any(feature = "sinks-socket", feature = "sinks-statsd"), unix))]
pub mod unix;
pub mod uri;
pub mod zstd;

use std::borrow::Cow;

pub use batch::{
    Batch, BatchConfig, BatchSettings, BatchSize, BulkSizeBasedDefaultBatchSettings, Merged,
    NoDefaultsBatchSettings, PushResult, RealtimeEventBasedDefaultBatchSettings,
    RealtimeSizeBasedDefaultBatchSettings, SinkBatchSettings, Unmerged,
};
pub use buffer::{
    json::{BoxedRawValue, JsonArrayBuffer},
    partition::Partition,
    vec::{EncodedLength, VecBuffer},
    Buffer, Compression, PartitionBuffer, PartitionInnerBuffer,
};
pub use builder::SinkBuilderExt;
pub use compressor::Compressor;
pub use normalizer::Normalizer;
pub use request_builder::{IncrementalRequestBuilder, RequestBuilder};
pub use service::{
    Concurrency, ServiceBuilderExt, TowerBatchedSink, TowerPartitionSink, TowerRequestConfig,
    TowerRequestLayer, TowerRequestSettings,
};
pub use sink::{BatchSink, PartitionBatchSink, StreamSink};
use snafu::Snafu;
pub use uri::UriSerde;
use vector_lib::{json_size::JsonSize, TimeZone};

use crate::event::EventFinalizers;
use chrono::{FixedOffset, Offset, Utc};

#[derive(Debug, Snafu)]
enum SinkBuildError {
    #[snafu(display("Missing host in address field"))]
    MissingHost,
    #[snafu(display("Missing port in address field"))]
    MissingPort,
}

#[derive(Debug)]
pub struct EncodedEvent<I> {
    pub item: I,
    pub finalizers: EventFinalizers,
    pub byte_size: usize,
    pub json_byte_size: JsonSize,
}

impl<I> EncodedEvent<I> {
    /// Create a trivial input with no metadata. This method will be
    /// removed when all sinks are converted.
    pub fn new(item: I, byte_size: usize, json_byte_size: JsonSize) -> Self {
        Self {
            item,
            finalizers: Default::default(),
            byte_size,
            json_byte_size,
        }
    }

    // This should be:
    // ```impl<F, I: From<F>> From<EncodedEvent<F>> for EncodedEvent<I>```
    // however, the compiler rejects that due to conflicting
    // implementations of `From` due to the generic
    // ```impl<T> From<T> for T```
    pub fn from<F>(that: EncodedEvent<F>) -> Self
    where
        I: From<F>,
    {
        Self {
            item: I::from(that.item),
            finalizers: that.finalizers,
            byte_size: that.byte_size,
            json_byte_size: that.json_byte_size,
        }
    }

    /// Remap the item using an adapter
    pub fn map<T>(self, doit: impl Fn(I) -> T) -> EncodedEvent<T> {
        EncodedEvent {
            item: doit(self.item),
            finalizers: self.finalizers,
            byte_size: self.byte_size,
            json_byte_size: self.json_byte_size,
        }
    }
}

/// Joins namespace with name via delimiter if namespace is present.
pub fn encode_namespace<'a>(
    namespace: Option<&str>,
    delimiter: char,
    name: impl Into<Cow<'a, str>>,
) -> String {
    let name = name.into();
    namespace
        .map(|namespace| format!("{}{}{}", namespace, delimiter, name))
        .unwrap_or_else(|| name.into_owned())
}

/// Marker trait for types that can hold a batch of events
pub trait ElementCount {
    fn element_count(&self) -> usize;
}

impl<T> ElementCount for Vec<T> {
    fn element_count(&self) -> usize {
        self.len()
    }
}

pub fn timezone_to_offset(tz: TimeZone) -> Option<FixedOffset> {
    match tz {
        TimeZone::Local => Some(*Utc::now().with_timezone(&chrono::Local).offset()),
        TimeZone::Named(tz) => Some(Utc::now().with_timezone(&tz).offset().fix()),
    }
}
