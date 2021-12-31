pub mod adaptive_concurrency;
pub mod batch;
pub mod buffer;
pub mod builder;
pub mod compressor;
pub mod encoding;
pub mod http;
pub mod normalizer;
pub mod partitioner;
pub mod processed_event;
pub mod request_builder;
pub mod retries;
pub mod service;
pub mod sink;
pub mod socket_bytes_sink;
pub mod statistic;
pub mod tcp;
#[cfg(test)]
pub mod test;
pub mod udp;
#[cfg(all(any(feature = "sinks-socket", feature = "sinks-statsd"), unix))]
pub mod unix;
pub mod uri;

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
use bytes::Bytes;
pub use compressor::Compressor;
use encoding::{EncodingConfig, EncodingConfiguration};
pub use normalizer::Normalizer;
pub use request_builder::{IncrementalRequestBuilder, RequestBuilder};
use serde::{Deserialize, Serialize};
pub use service::{
    Concurrency, ServiceBuilderExt, TowerBatchedSink, TowerPartitionSink, TowerRequestConfig,
    TowerRequestLayer, TowerRequestSettings,
};
pub use sink::{BatchSink, PartitionBatchSink, StreamSink};
use snafu::Snafu;
pub use uri::UriSerde;

use crate::event::{Event, EventFinalizers};

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
}

impl<I> EncodedEvent<I> {
    /// Create a trivial input with no metadata. This method will be
    /// removed when all sinks are converted.
    pub fn new(item: I, byte_size: usize) -> Self {
        Self {
            item,
            finalizers: Default::default(),
            byte_size,
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
        }
    }

    /// Remap the item using an adapter
    pub fn map<T>(self, doit: impl Fn(I) -> T) -> EncodedEvent<T> {
        EncodedEvent {
            item: doit(self.item),
            finalizers: self.finalizers,
            byte_size: self.byte_size,
        }
    }
}

/**
 * Enum representing different ways to encode events as they are sent into a Sink.
 */
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

/**
* Encodes the given event into raw bytes that can be sent into a Sink, according to
* the given encoding. If there are any errors encoding the event, logs a warning
* and returns None.
**/
pub fn encode_log(mut event: Event, encoding: &EncodingConfig<Encoding>) -> Option<Bytes> {
    encoding.apply_rules(&mut event);
    let log = event.into_log();

    let b = match encoding.codec() {
        Encoding::Json => serde_json::to_vec(&log),
        Encoding::Text => {
            let bytes = log
                .get(crate::config::log_schema().message_key())
                .map(|v| v.as_bytes().to_vec())
                .unwrap_or_default();
            Ok(bytes)
        }
    };

    b.map(|mut b| {
        b.push(b'\n');
        Bytes::from(b)
    })
    .map_err(|error| error!(message = "Unable to encode.", %error))
    .ok()
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

impl ElementCount for serde_json::Value {
    fn element_count(&self) -> usize {
        1
    }
}
