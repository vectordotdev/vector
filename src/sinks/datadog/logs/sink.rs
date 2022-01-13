use std::{
    fmt::Debug,
    io::{self, Write},
    num::NonZeroUsize,
    sync::Arc,
};

use async_trait::async_trait;
use futures::{stream::BoxStream, StreamExt};
use snafu::Snafu;
use tower::Service;
use vector_core::{
    buffers::Acker,
    config::{log_schema, LogSchema},
    event::{Event, EventFinalizers, Finalizable, Value},
    partition::Partitioner,
    sink::StreamSink,
    stream::{BatcherSettings, DriverResponse},
    ByteSizeOf,
};

use super::{config::MAX_PAYLOAD_BYTES, service::LogApiRequest};
use crate::{
    config::SinkContext,
    sinks::util::{
        encoding::{Encoder, EncodingConfigFixed, StandardEncodings},
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
    encoding: EncodingConfigFixed<DatadogLogsJsonEncoding>,
    service: S,
    context: SinkContext,
    batch_settings: BatcherSettings,
    compression: Option<Compression>,
    default_api_key: Arc<str>,
}

impl<S> LogSinkBuilder<S> {
    pub fn new(
        service: S,
        context: SinkContext,
        default_api_key: Arc<str>,
        batch_settings: BatcherSettings,
    ) -> Self {
        Self {
            encoding: Default::default(),
            service,
            context,
            default_api_key,
            batch_settings,
            compression: None,
        }
    }

    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
    pub fn encoding(mut self, encoding: EncodingConfigFixed<DatadogLogsJsonEncoding>) -> Self {
        self.encoding = encoding;
        self
    }

    pub const fn compression(mut self, compression: Compression) -> Self {
        self.compression = Some(compression);
        self
    }

    pub fn build(self) -> LogSink<S> {
        LogSink {
            default_api_key: self.default_api_key,
            encoding: self.encoding,
            acker: self.context.acker(),
            service: self.service,
            batch_settings: self.batch_settings,
            compression: self.compression.unwrap_or_default(),
        }
    }
}

pub struct LogSink<S> {
    /// The default Datadog API key to use
    ///
    /// In some instances an `Event` will come in on the stream with an
    /// associated API key. That API key is the one it'll get batched up by but
    /// otherwise we will see `Event` instances with no associated key. In that
    /// case we batch them by this default.
    default_api_key: Arc<str>,
    /// The ack system for this sink to vector's buffer mechanism
    acker: Acker,
    /// The API service
    service: S,
    /// The encoding of payloads
    encoding: EncodingConfigFixed<DatadogLogsJsonEncoding>,
    /// The compression technique to use when building the request body
    compression: Compression,
    /// Batch settings: timeout, max events, max bytes, etc.
    batch_settings: BatcherSettings,
}

/// Customized encoding specific to the Datadog Logs sink, as the logs API only accepts JSON encoded
/// log lines, and requires some specific normalization of certain event fields.
#[derive(Clone, Debug, PartialEq)]
pub struct DatadogLogsJsonEncoding {
    log_schema: &'static LogSchema,
    inner: StandardEncodings,
}

impl Default for DatadogLogsJsonEncoding {
    fn default() -> Self {
        DatadogLogsJsonEncoding {
            log_schema: log_schema(),
            inner: StandardEncodings::Json,
        }
    }
}

impl Encoder<Vec<Event>> for DatadogLogsJsonEncoding {
    fn encode_input(&self, mut input: Vec<Event>, writer: &mut dyn io::Write) -> io::Result<usize> {
        for event in input.iter_mut() {
            let log = event.as_mut_log();
            log.rename_key_flat(self.log_schema.message_key(), "message");
            log.rename_key_flat(self.log_schema.host_key(), "host");
            if let Some(Value::Timestamp(ts)) = log.remove(self.log_schema.timestamp_key()) {
                log.insert_flat("timestamp", Value::Integer(ts.timestamp_millis()));
            }
        }

        self.inner.encode_input(input, writer)
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

struct LogRequestBuilder {
    default_api_key: Arc<str>,
    encoding: EncodingConfigFixed<DatadogLogsJsonEncoding>,
    compression: Compression,
}

impl RequestBuilder<(Option<Arc<str>>, Vec<Event>)> for LogRequestBuilder {
    type Metadata = (Arc<str>, usize, EventFinalizers, usize);
    type Events = Vec<Event>;
    type Encoder = EncodingConfigFixed<DatadogLogsJsonEncoding>;
    type Payload = Vec<u8>;
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
        let mut compressor = Compressor::from(self.compression);
        let _ = compressor.write_all(&buf)?;

        Ok(compressor.into_inner())
    }

    fn build_request(&self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let (api_key, batch_size, finalizers, events_byte_size) = metadata;
        LogApiRequest {
            batch_size,
            api_key,
            compression: self.compression,
            body: payload,
            finalizers,
            events_byte_size,
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
        let request_builder = LogRequestBuilder {
            default_api_key,
            encoding: self.encoding,
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
