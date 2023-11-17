use std::{fmt::Debug, io, sync::Arc};

use serde::{ser::SerializeSeq, Serializer};
use snafu::Snafu;
use vector_lib::lookup::event_path;

use super::{config::MAX_PAYLOAD_BYTES, service::LogApiRequest};
use crate::sinks::{
    prelude::*,
    util::{http::HttpJsonBatchSizer, Compressor},
};
#[derive(Default)]
struct EventPartitioner;

impl Partitioner for EventPartitioner {
    type Item = Event;
    type Key = Option<Arc<str>>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        item.metadata().datadog_api_key()
    }
}

#[derive(Debug)]
pub struct LogSinkBuilder<S> {
    transformer: Transformer,
    service: S,
    batch_settings: BatcherSettings,
    compression: Option<Compression>,
    default_api_key: Arc<str>,
    protocol: String,
}

impl<S> LogSinkBuilder<S> {
    pub fn new(
        transformer: Transformer,
        service: S,
        default_api_key: Arc<str>,
        batch_settings: BatcherSettings,
        protocol: String,
    ) -> Self {
        Self {
            transformer,
            service,
            default_api_key,
            batch_settings,
            compression: None,
            protocol,
        }
    }

    pub const fn compression(mut self, compression: Compression) -> Self {
        self.compression = Some(compression);
        self
    }

    pub fn build(self) -> LogSink<S> {
        LogSink {
            default_api_key: self.default_api_key,
            transformer: self.transformer,
            service: self.service,
            batch_settings: self.batch_settings,
            compression: self.compression.unwrap_or_default(),
            protocol: self.protocol,
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
    /// The API service
    service: S,
    /// The encoding of payloads
    transformer: Transformer,
    /// The compression technique to use when building the request body
    compression: Compression,
    /// Batch settings: timeout, max events, max bytes, etc.
    batch_settings: BatcherSettings,
    /// The protocol name
    protocol: String,
}

fn normalize_event(event: &mut Event) {
    let log = event.as_mut_log();
    let message_path = log
        .message_path()
        .expect("message is required (make sure the \"message\" semantic meaning is set)")
        .clone();
    log.rename_key(&message_path, event_path!("message"));

    if let Some(host_path) = log.host_path().cloned().as_ref() {
        log.rename_key(host_path, event_path!("hostname"));
    }

    let message_path = log
        .timestamp_path()
        .expect("timestamp is required (make sure the \"timestamp\" semantic meaning is set)")
        .clone();
    if let Some(Value::Timestamp(ts)) = log.remove(&message_path) {
        log.insert(
            event_path!("timestamp"),
            Value::Integer(ts.timestamp_millis()),
        );
    }
}

#[derive(Debug, Snafu)]
pub enum RequestBuildError {
    #[snafu(display("Encoded payload is greater than the max limit."))]
    PayloadTooBig { events_that_fit: usize },
    #[snafu(display("Failed to build payload with error: {}", error))]
    Io { error: std::io::Error },
    #[snafu(display("Failed to serialize payload with error: {}", error))]
    Json { error: serde_json::Error },
}

impl From<io::Error> for RequestBuildError {
    fn from(error: io::Error) -> RequestBuildError {
        RequestBuildError::Io { error }
    }
}

impl From<serde_json::Error> for RequestBuildError {
    fn from(error: serde_json::Error) -> RequestBuildError {
        RequestBuildError::Json { error }
    }
}

struct LogRequestBuilder {
    default_api_key: Arc<str>,
    transformer: Transformer,
    compression: Compression,
}

struct CountingWrite<'a, T> {
    inner: T,
    count: &'a std::cell::Cell<usize>,
}

impl<'a, T> CountingWrite<'a, T> {
    fn into_inner(self) -> T {
        self.inner
    }
}

impl<'a, T: std::io::Write> std::io::Write for CountingWrite<'a, T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.count.set(self.count.get() + buf.len());
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl LogRequestBuilder {
    fn build_request(
        &self,
        mut events: Vec<Event>,
        api_key: Arc<str>,
    ) -> Result<Vec<LogApiRequest>, RequestBuildError> {
        // TODO: this estimated json size seems seems redundant with the one in
        // RequestMetadataBuilder::from_events
        let mut byte_size = telemetry().create_request_count_byte_size();
        let mut total_estimated = 0;
        for event in events.iter_mut() {
            normalize_event(event);
            self.transformer.transform(event);
            let estimated = event.estimated_json_encoded_size_of();
            byte_size.add_event(event, estimated);
            total_estimated += estimated.get();
        }

        let mut batches = vec![events];
        let mut requests = Vec::new();

        while let Some(mut events) = batches.pop() {
            if events.is_empty() {
                continue;
            }
            match try_serialize(&events, total_estimated) {
                Ok(buf) => {
                    let request =
                        self.finish_request(buf, events, byte_size.clone(), api_key.clone())?;
                    requests.push(request);
                }
                Err(RequestBuildError::PayloadTooBig { events_that_fit }) => {
                    if events_that_fit == 0 {
                        // first event was too large for whole request
                        let _too_big = events.pop();
                        // TODO: emit dropped event

                        batches.push(events);
                    } else {
                        let next = events.split_off(events_that_fit);
                        batches.push(events);
                        batches.push(next);
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Ok(requests)
    }

    fn finish_request(
        &self,
        buf: Vec<u8>,
        mut events: Vec<Event>,
        byte_size: GroupedCountByteSize,
        api_key: Arc<str>,
    ) -> Result<LogApiRequest, RequestBuildError> {
        let n_events = events.len();
        let uncompressed_size = buf.len();

        // Now just compress it like normal.
        let mut compressor = Compressor::from(self.compression);
        write_all(&mut compressor, n_events, &buf)?;
        let bytes = compressor.into_inner().freeze();

        let finalizers = events.take_finalizers();
        let request_metadata_builder = RequestMetadataBuilder::from_events(&events);

        let payload = if self.compression.is_compressed() {
            EncodeResult::compressed(bytes, uncompressed_size, byte_size)
        } else {
            EncodeResult::uncompressed(bytes, byte_size)
        };

        Ok::<_, RequestBuildError>(LogApiRequest {
            api_key,
            finalizers,
            compression: self.compression,
            metadata: request_metadata_builder.build(&payload),
            uncompressed_size: payload.uncompressed_byte_size,
            body: payload.into_payload(),
        })
    }
}

fn try_serialize(events: &[Event], total_estimated: usize) -> Result<Vec<u8>, RequestBuildError> {
    let byte_count = std::cell::Cell::new(0);
    let w = CountingWrite {
        inner: Vec::with_capacity(total_estimated),
        count: &byte_count,
    };

    let mut events_that_fit = 0;
    let mut ser = serde_json::Serializer::new(w);
    let mut seq = ser.serialize_seq(Some(events.len()))?;
    for event in events.iter() {
        seq.serialize_element(event.as_log())?;
        if byte_count.get() < MAX_PAYLOAD_BYTES {
            events_that_fit += 1;
        } else {
            return Err(RequestBuildError::PayloadTooBig { events_that_fit });
        }
    }
    seq.end()?;

    let buf = ser.into_inner().into_inner();
    Ok(buf)
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

        let partitioner = EventPartitioner;
        let batch_settings = self.batch_settings;
        let builder = Arc::new(LogRequestBuilder {
            default_api_key,
            transformer: self.transformer,
            compression: self.compression,
        });

        let input = input.batched_partitioned(partitioner, || {
            batch_settings.as_item_size_config(HttpJsonBatchSizer)
        });
        input
            .concurrent_map(default_request_builder_concurrency_limit(), move |input| {
                let builder = Arc::clone(&builder);

                Box::pin(async move {
                    let (api_key, events) = input;
                    let api_key = api_key.unwrap_or_else(|| Arc::clone(&builder.default_api_key));

                    builder.build_request(events, api_key)
                })
            })
            .filter_map(|request| async move {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(reqs) => Some(futures::stream::iter(reqs)),
                }
            })
            .flatten()
            .into_driver(self.service)
            .protocol(self.protocol)
            .run()
            .await
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
