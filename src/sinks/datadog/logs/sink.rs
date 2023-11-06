use std::{fmt::Debug, io, sync::Arc};

use bytes::Bytes;
use snafu::Snafu;
use vector_lib::{lookup::event_path, stream::batcher::limiter::ItemBatchSize};

use super::{config::MAX_PAYLOAD_BYTES, service::LogApiRequest};
use crate::sinks::{prelude::*, util::Compressor};

#[derive(Default)]
struct EventPartitioner;

impl Partitioner for EventPartitioner {
    type Item = EncodedEvent;
    type Key = Option<Arc<str>>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        item.original.metadata().datadog_api_key()
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

fn map_event(event: &mut Event) {
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
    compression: Compression,
}

impl LogRequestBuilder {
    fn encode_events(
        &self,
        events: Vec<EncodedEvent>,
    ) -> Result<EncodeResult<Bytes>, RequestBuildError> {
        // We need to first serialize the payload separately so that we can figure out how big it is
        // before compression.  The Datadog Logs API has a limit on uncompressed data, so we can't
        // use the default implementation of this method.
        //
        // TODO: We should probably make `build_request` fallible itself, because then this override of `encode_events`
        // wouldn't even need to exist, and we could handle it in `build_request` which is required by all implementors.
        //
        // On the flip side, it would mean that we'd potentially be compressing payloads that we would inevitably end up
        // rejecting anyways, which is meh. This might be a signal that the true "right" fix is to actually switch this
        // sink to incremental encoding and simply put up with suboptimal batch sizes if we need to end up splitting due
        // to (un)compressed size limitations.
        let mut byte_size = telemetry().create_request_count_byte_size();
        let n_events = events.len();
        let mut payload = Vec::with_capacity(n_events);
        for e in events {
            byte_size += e.byte_size;
            payload.push(e.encoded);
        }

        let buf = serde_json::to_vec(&payload).expect("serializing to memory");
        let uncompressed_size = buf.len();
        if uncompressed_size > MAX_PAYLOAD_BYTES {
            return Err(RequestBuildError::PayloadTooBig);
        }

        // Now just compress it like normal.
        let mut compressor = Compressor::from(self.compression);
        write_all(&mut compressor, n_events, &buf)?;
        let bytes = compressor.into_inner().freeze();

        if self.compression.is_compressed() {
            Ok(EncodeResult::compressed(
                bytes,
                uncompressed_size,
                byte_size,
            ))
        } else {
            Ok(EncodeResult::uncompressed(bytes, byte_size))
        }
    }
}

struct ActualJsonSize;

impl ItemBatchSize<EncodedEvent> for ActualJsonSize {
    fn size(&self, item: &EncodedEvent) -> usize {
        item.encoded.get().len() + 1 // one for comma
    }
}

struct EncodedEvent {
    original: Event,
    encoded: Box<serde_json::value::RawValue>,
    byte_size: GroupedCountByteSize,
}

impl Finalizable for EncodedEvent {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.original.take_finalizers()
    }
}

// only for RequestMetadataBuilder::from_events
impl EstimatedJsonEncodedSizeOf for EncodedEvent {
    fn estimated_json_encoded_size_of(&self) -> JsonSize {
        // we could use the actual json size here, but opting to stay consistent
        self.original.estimated_json_encoded_size_of()
    }
}

// only for RequestMetadataBuilder::from_events
impl ByteSizeOf for EncodedEvent {
    fn allocated_bytes(&self) -> usize {
        self.original.allocated_bytes()
    }
}

impl GetEventCountTags for EncodedEvent {
    fn get_tags(&self) -> TaggedEventsSent {
        self.original.get_tags()
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

        let partitioner = EventPartitioner;
        let batch_settings = self.batch_settings;
        let transformer = Arc::new(self.transformer);
        let builder = Arc::new(LogRequestBuilder {
            default_api_key,
            compression: self.compression,
        });

        let input = input
            .ready_chunks(1024)
            .concurrent_map(default_request_builder_concurrency_limit(), move |events| {
                let transformer = Arc::clone(&transformer);
                Box::pin(std::future::ready(futures::stream::iter(
                    events.into_iter().map(move |mut event| {
                        let original = event.clone();

                        map_event(&mut event);
                        transformer.transform(&mut event);

                        let mut byte_size = telemetry().create_request_count_byte_size();
                        byte_size.add_event(&event, event.estimated_json_encoded_size_of());
                        let encoded = serde_json::value::to_raw_value(&event.as_log())
                            .expect("serializing to memory");

                        EncodedEvent {
                            original,
                            encoded,
                            byte_size,
                        }
                    }),
                )))
            })
            .flatten();

        input
            .batched_partitioned(partitioner, || {
                batch_settings.as_item_size_config(ActualJsonSize)
            })
            .concurrent_map(default_request_builder_concurrency_limit(), move |input| {
                let builder = Arc::clone(&builder);

                Box::pin(async move {
                    let (api_key, mut events) = input;
                    let finalizers = events.take_finalizers();
                    let api_key = api_key.unwrap_or_else(|| Arc::clone(&builder.default_api_key));
                    let request_metadata_builder = RequestMetadataBuilder::from_events(&events);

                    let payload = builder.encode_events(events)?;

                    Ok::<_, RequestBuildError>(LogApiRequest {
                        api_key,
                        finalizers,
                        compression: builder.compression,
                        metadata: request_metadata_builder.build(&payload),
                        uncompressed_size: payload.uncompressed_byte_size,
                        body: payload.into_payload(),
                    })
                })
            })
            .filter_map(|request| async move {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
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
