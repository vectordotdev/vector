use std::{
    fmt::Debug,
    io::{self, Write},
    num::NonZeroUsize,
    sync::Arc,
};

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use codecs::{encoding::Framer, JsonSerializer, NewlineDelimitedEncoder};
use futures::{stream::BoxStream, StreamExt};
use snafu::Snafu;
use tokio_util::codec::Encoder as _;
use tower::Service;
use value::Value;
use vector_core::{
    buffers::Acker,
    config::log_schema,
    event::{Event, EventFinalizers, Finalizable, LogEvent},
    partition::Partitioner,
    sink::StreamSink,
    stream::{BatcherSettings, DriverResponse},
    ByteSizeOf,
};

use super::{config::MAX_PAYLOAD_BYTES, service::LogApiRequest};
use crate::{
    codecs::{Encoder, EncodingConfig},
    config::SinkContext,
    sinks::util::{
        encoding::{Encoder as _, Transformer},
        Compression, Compressor, RequestBuilder, SinkBuilderExt,
    },
};

#[derive(Default)]
struct EventPartitioner;

impl Partitioner for EventPartitioner {
    type Item = Event;
    type Key = Option<Arc<str>>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        item.metadata().datadog_api_key().clone()
    }
}

#[derive(Debug)]
pub struct LogSinkBuilder<S> {
    encoding: EncodingConfig,
    service: S,
    context: SinkContext,
    batch_settings: BatcherSettings,
    compression: Option<Compression>,
    default_api_key: Arc<str>,
}

impl<S> LogSinkBuilder<S> {
    pub fn new(
        encoding: EncodingConfig,
        service: S,
        context: SinkContext,
        default_api_key: Arc<str>,
        batch_settings: BatcherSettings,
    ) -> Self {
        Self {
            encoding,
            service,
            context,
            default_api_key,
            batch_settings,
            compression: None,
        }
    }

    pub const fn compression(mut self, compression: Compression) -> Self {
        self.compression = Some(compression);
        self
    }

    pub fn build(self) -> LogSink<S> {
        LogSink {
            default_api_key: self.default_api_key,
            transformer: self.encoding.transformer(),
            encoder: Encoder::<()>::new(self.encoding.config().build()),
            acker: self.context.acker(),
            service: self.service,
            batch_settings: self.batch_settings,
            compression: self.compression.unwrap_or_default(),
        }
    }
}

pub struct LogSink<S> {
    /// The default Datadog API key to use.
    ///
    /// In some instances an `Event` will come in on the stream with an
    /// associated API key. That API key is the one it'll get batched up by but
    /// otherwise we will see `Event` instances with no associated key. In that
    /// case we batch them by this default.
    default_api_key: Arc<str>,
    /// The ack system for this sink to vector's buffer mechanism.
    acker: Acker,
    /// The API service.
    service: S,
    /// The transformer to reshape events before serialization.
    transformer: Transformer,
    /// The encoder of payloads.
    encoder: Encoder<()>,
    /// The compression technique to use when building the request body.
    compression: Compression,
    /// Batch settings: timeout, max events, max bytes, etc.
    batch_settings: BatcherSettings,
}

struct DatadogEncoder {
    transformer: Transformer,
    encoder: Encoder<()>,
}

impl crate::sinks::util::encoding::Encoder<Vec<Event>> for DatadogEncoder {
    fn encode_input(&self, input: Vec<Event>, writer: &mut dyn io::Write) -> io::Result<usize> {
        let outer_encoder = (
            Transformer::default(),
            Encoder::<Framer>::new(
                NewlineDelimitedEncoder::new().into(),
                JsonSerializer::new().into(),
            ),
        );
        let mut encoder = self.encoder.clone();

        let input = input
            .into_iter()
            .flat_map(|mut event| {
                let log = event.as_mut_log();

                let ddsource = log.remove("ddsource");
                let ddtags = log.remove("ddtags");
                let hostname = log.remove(log_schema().host_key());
                let timestamp = log
                    .remove(log_schema().timestamp_key())
                    .and_then(|timestamp| {
                        timestamp
                            .as_timestamp()
                            .map(|timestamp| timestamp.timestamp_millis())
                    });
                let service = log.remove("service");
                let metadata = std::mem::take(log.metadata_mut());

                self.transformer.transform(&mut event);

                let mut bytes = BytesMut::new();
                encoder.encode(event, &mut bytes).ok()?;

                let message = Value::Bytes(bytes.freeze());

                let mut outer = LogEvent::new_with_metadata(metadata);
                if let Some(ddsource) = ddsource {
                    outer.insert_flat("ddsource", ddsource);
                }
                if let Some(ddtags) = ddtags {
                    outer.insert_flat("ddtags", ddtags);
                }
                if let Some(hostname) = hostname {
                    outer.insert_flat("hostname", hostname);
                }
                if let Some(timestamp) = timestamp {
                    outer.insert_flat("timestamp", timestamp);
                }
                if let Some(service) = service {
                    outer.insert_flat("service", service);
                }
                outer.insert_flat("message", message);

                Some(Event::from(outer))
            })
            .collect::<Vec<_>>();

        outer_encoder.encode_input(input, writer)
    }
}

#[derive(Debug, Snafu)]
pub enum RequestBuildError {
    #[snafu(display("Encoded payload is greater than the max limit."))]
    PayloadTooBig,
    #[snafu(display("Failed to build payload with error: {}", error))]
    Io { error: std::io::Error },
}

impl From<io::Error> for RequestBuildError {
    fn from(error: io::Error) -> RequestBuildError {
        RequestBuildError::Io { error }
    }
}

/// The payload for the log request needs to store the length of the uncompressed
/// data so we can report the metric on successful delivery.
struct LogRequestPayload {
    bytes: Bytes,
    uncompressed_size: usize,
}

impl From<Bytes> for LogRequestPayload {
    fn from(bytes: Bytes) -> Self {
        let uncompressed_size = bytes.len();
        Self {
            bytes,
            uncompressed_size,
        }
    }
}

struct LogRequestBuilder {
    default_api_key: Arc<str>,
    encoding: DatadogEncoder,
    compression: Compression,
}

impl RequestBuilder<(Option<Arc<str>>, Vec<Event>)> for LogRequestBuilder {
    type Metadata = (Arc<str>, usize, EventFinalizers, usize);
    type Events = Vec<Event>;
    type Encoder = DatadogEncoder;
    type Payload = LogRequestPayload;
    type Request = LogApiRequest;
    type Error = RequestBuildError;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoding
    }

    fn split_input(&self, input: (Option<Arc<str>>, Vec<Event>)) -> (Self::Metadata, Self::Events) {
        let (api_key, mut events) = input;
        let events_len = events.len();
        let finalizers = events.take_finalizers();
        let events_byte_size = events.size_of();

        let api_key = api_key.unwrap_or_else(|| Arc::clone(&self.default_api_key));
        ((api_key, events_len, finalizers, events_byte_size), events)
    }

    fn encode_events(&self, events: Self::Events) -> Result<Self::Payload, Self::Error> {
        // We need to first serialize the payload separately so that we can figure out how big it is
        // before compression.  The Datadog Logs API has a limit on uncompressed data, so we can't
        // use the default implementation of this method.
        let mut buf = Vec::new();
        let n = self.encoder().encode_input(events, &mut buf)?;
        if n > MAX_PAYLOAD_BYTES {
            return Err(RequestBuildError::PayloadTooBig);
        }

        // Now just compress it like normal.
        let uncompressed_size = buf.len();
        let mut compressor = Compressor::from(self.compression);
        let _ = compressor.write_all(&buf)?;
        let bytes = compressor.into_inner().freeze();

        Ok(LogRequestPayload {
            bytes,
            uncompressed_size,
        })
    }

    fn build_request(&self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let (api_key, batch_size, finalizers, events_byte_size) = metadata;
        LogApiRequest {
            batch_size,
            api_key,
            compression: self.compression,
            body: payload.bytes,
            finalizers,
            events_byte_size,
            uncompressed_size: payload.uncompressed_size,
        }
    }
}

impl<S> LogSink<S>
where
    S: Service<LogApiRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: Debug + Into<crate::Error> + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let default_api_key = Arc::clone(&self.default_api_key);

        let partitioner = EventPartitioner::default();

        let builder_limit = NonZeroUsize::new(64);
        let encoding = DatadogEncoder {
            transformer: self.transformer,
            encoder: self.encoder,
        };
        let request_builder = LogRequestBuilder {
            default_api_key,
            encoding,
            compression: self.compression,
        };

        let sink = input
            .batched_partitioned(partitioner, self.batch_settings)
            .request_builder(builder_limit, request_builder)
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("Failed to build Datadog Logs request: {:?}.", e);
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service, self.acker);

        sink.run().await
    }
}

#[async_trait]
impl<S> StreamSink<Event> for LogSink<S>
where
    S: Service<LogApiRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
