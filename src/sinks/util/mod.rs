pub mod auto_concurrency;
pub mod batch;
pub mod buffer;
pub mod encoding;
pub mod http;
pub mod retries;
#[cfg(feature = "rusoto_core")]
pub mod rusoto;
pub mod service;
pub mod sink;
pub mod statistic;
pub mod tcp;
#[cfg(test)]
pub mod test;
pub mod udp;
#[cfg(all(feature = "sinks-socket", unix))]
pub mod unix;
pub mod uri;

use crate::event::Event;
use bytes::Bytes;
use encoding::{EncodingConfig, EncodingConfiguration};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::borrow::Cow;
use string_cache::DefaultAtom as Atom;

pub use batch::{Batch, BatchConfig, BatchSettings, BatchSize, PushResult};
pub use buffer::json::{BoxedRawValue, JsonArrayBuffer};
pub use buffer::metrics::{MetricBuffer, MetricEntry};
pub use buffer::partition::Partition;
pub use buffer::vec::{EncodedLength, VecBuffer};
pub use buffer::{Buffer, Compression, PartitionBuffer, PartitionInnerBuffer};
pub use service::{
    InFlightLimit, ServiceBuilderExt, TowerBatchedSink, TowerRequestConfig, TowerRequestLayer,
    TowerRequestSettings,
};
pub use sink::{BatchSink, PartitionBatchSink, StreamSink, StreamSinkOld};
pub use uri::UriSerde;

#[derive(Debug, Snafu)]
enum SinkBuildError {
    #[snafu(display("Missing host in address field"))]
    MissingHost,
    #[snafu(display("Missing port in address field"))]
    MissingPort,
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
pub fn encode_event(mut event: Event, encoding: &EncodingConfig<Encoding>) -> Option<Bytes> {
    encoding.apply_rules(&mut event);
    let log = event.into_log();

    let b = match encoding.codec() {
        Encoding::Json => serde_json::to_vec(&log),
        Encoding::Text => {
            let bytes = log
                .get(&Atom::from(crate::config::log_schema().message_key()))
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

/// Joins namespace with name via delimiter if namespace is present and not empty.
pub fn encode_namespace<'a>(
    namespace: Option<&str>,
    delimiter: char,
    name: impl Into<Cow<'a, str>>,
) -> String {
    let name = name.into();
    match namespace {
        Some(namespace) if namespace.is_empty() => {
            warn!("Dropping empty namespace. This feature has been deprecated, and could be removed in the future.");
            name.into_owned()
        }
        Some(namespace) => format!("{}{}{}", namespace, delimiter, name),
        _ => name.into_owned(),
    }
}
