use super::service::LogApiRequest;
use crate::config::SinkContext;
use crate::sinks::datadog::logs::config::Encoding;
use crate::sinks::util::buffer::GZIP_FAST;
use crate::sinks::util::encoding::{EncodingConfigWithDefault, EncodingConfiguration};
use crate::sinks::util::Compression;
use async_trait::async_trait;
use flate2::write::GzEncoder;
use futures::future::FutureExt;
use futures::stream::BoxStream;
use futures::{StreamExt, TryFutureExt};
use futures_util::stream::FuturesUnordered;
use metrics::gauge;
use snafu::Snafu;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::BuildHasherDefault;
use std::io::Write;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{channel, Receiver};
use tokio::sync::oneshot;
use tokio::{pin, select};
use tower::{Service, ServiceExt};
use tracing_futures::Instrument;
use twox_hash::XxHash64;
use vector_core::buffers::Acker;
use vector_core::config::{log_schema, LogSchema};
use vector_core::event::{Event, EventFinalizers, EventStatus, Finalizable, Value};
use vector_core::partition::Partitioner;
use vector_core::sink::StreamSink;
use vector_core::stream::batcher::Batcher;

const MAX_PAYLOAD_ARRAY: usize = 1_000;
const MAX_PAYLOAD_BYTES: usize = 5_000_000;
// The Datadog API has a hard limit of 5MB for uncompressed payloads. Above this
// threshold the API will toss results. We previously serialized Events as they
// came in -- a very CPU intensive process -- and to avoid that we only batch up
// to 750KB below the max and then build our payloads. This does mean that in
// some situations we'll kick out over-large payloads -- for instance, a string
// of escaped double-quotes -- but we believe this should be very rare in
// practice.
const BATCH_GOAL_BYTES: usize = 4_250_000;
const BATCH_DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Default)]
struct EventPartitioner {}

impl Partitioner for EventPartitioner {
    type Item = Event;
    type Key = Option<Arc<str>>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        item.metadata().datadog_api_key().clone()
    }
}

#[derive(Debug)]
pub struct LogSinkBuilder<S> {
    encoding: EncodingConfigWithDefault<Encoding>,
    service: S,
    context: SinkContext,
    timeout: Option<Duration>,
    compression: Option<Compression>,
    default_api_key: Arc<str>,
    log_schema: Option<&'static LogSchema>,
}

impl<S> LogSinkBuilder<S> {
    pub fn new(service: S, context: SinkContext, default_api_key: Arc<str>) -> Self {
        Self {
            encoding: Default::default(),
            service,
            context,
            default_api_key,
            timeout: None,
            compression: None,
            log_schema: None,
        }
    }

    pub const fn log_schema(mut self, log_schema: &'static LogSchema) -> Self {
        self.log_schema = Some(log_schema);
        self
    }

    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
    pub fn encoding(mut self, encoding: EncodingConfigWithDefault<Encoding>) -> Self {
        self.encoding = encoding;
        self
    }

    pub const fn compression(mut self, compression: Compression) -> Self {
        self.compression = Some(compression);
        self
    }

    pub const fn batch_timeout(mut self, duration: Option<Duration>) -> Self {
        self.timeout = duration;
        self
    }

    pub fn build(self) -> LogSink<S> {
        LogSink {
            default_api_key: self.default_api_key,
            encoding: Some(self.encoding),
            acker: Some(self.context.acker()),
            service: Some(self.service),
            timeout: self.timeout.unwrap_or(BATCH_DEFAULT_TIMEOUT),
            compression: self.compression.unwrap_or_default(),
            log_schema: self.log_schema.unwrap_or_else(|| log_schema()),
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
    acker: Option<Acker>,
    /// The API service
    service: Option<S>,
    /// The encoding of payloads
    ///
    /// This struct always generates JSON payloads. However we do, technically,
    /// allow the user to set the encoding to a single value -- JSON -- and this
    /// encoding comes with rules on sanitizing the payload which must be
    /// applied.
    encoding: Option<EncodingConfigWithDefault<Encoding>>,
    /// The compression technique to use when building the request body
    compression: Compression,
    /// The total duration before a flush is forced
    ///
    /// This value sets the duration that is allowed to ellapse prior to a flush
    /// of all buffered `Event` instances.
    timeout: Duration,
    /// The `LogSchema` for this instance
    log_schema: &'static LogSchema,
}

impl<S> LogSink<S> {
    pub fn new(service: S, context: SinkContext, default_api_key: Arc<str>) -> LogSinkBuilder<S> {
        LogSinkBuilder::new(service, context, default_api_key)
    }
}

struct RequestBuilder {
    encoding: EncodingConfigWithDefault<Encoding>,
    compression: Compression,
    log_schema_message_key: &'static str,
    log_schema_timestamp_key: &'static str,
    log_schema_host_key: &'static str,
}

#[derive(Debug, Snafu)]
pub enum RequestBuildError {
    #[snafu(display("Encoded payload is greater than the max limit."))]
    PayloadTooBig,
    #[snafu(display("Failed to build payload with error: {}", error))]
    Io { error: std::io::Error },
}

impl RequestBuilder {
    fn new(
        encoding: EncodingConfigWithDefault<Encoding>,
        compression: Compression,
        log_schema: &'static LogSchema,
    ) -> Self {
        Self {
            encoding,
            compression,
            log_schema_message_key: log_schema.message_key(),
            log_schema_timestamp_key: log_schema.timestamp_key(),
            log_schema_host_key: log_schema.host_key(),
        }
    }

    #[inline]
    fn dissect_batch(&self, batch: Vec<Event>) -> (Vec<BTreeMap<String, Value>>, EventFinalizers) {
        let mut members: Vec<BTreeMap<String, Value>> = Vec::with_capacity(batch.len());
        let mut finalizers: EventFinalizers = EventFinalizers::default();
        for mut event in batch.into_iter() {
            {
                let log = event.as_mut_log();
                log.rename_key_flat(self.log_schema_message_key, "message");
                log.rename_key_flat(self.log_schema_timestamp_key, "date");
                log.rename_key_flat(self.log_schema_host_key, "host");
                self.encoding.apply_rules(&mut event);
            }

            let (fields, mut metadata) = event.into_log().into_parts();
            members.push(fields);
            finalizers.merge(metadata.take_finalizers());
        }
        (members, finalizers)
    }

    fn build(
        &self,
        api_key: Arc<str>,
        batch: Vec<Event>,
    ) -> Result<LogApiRequest, RequestBuildError> {
        let (members, finalizers) = self.dissect_batch(batch);

        let total_members = members.len();
        assert!(total_members <= MAX_PAYLOAD_ARRAY);
        let body: Vec<u8> = serde_json::to_vec(&members).expect("failed to encode to json");
        let serialized_payload_bytes_len = body.len();
        if serialized_payload_bytes_len > MAX_PAYLOAD_BYTES {
            return Err(RequestBuildError::PayloadTooBig);
        }
        metrics::histogram!(
            "encoded_payload_size_bytes",
            serialized_payload_bytes_len as f64
        );
        let (encoded_body, is_compressed) = match self.compression {
            Compression::None => (body, false),
            Compression::Gzip(level) => {
                let level = level.unwrap_or(GZIP_FAST);
                let mut encoder = GzEncoder::new(
                    Vec::with_capacity(serialized_payload_bytes_len),
                    flate2::Compression::new(level as u32),
                );

                encoder
                    .write_all(&body)
                    .map_err(|error| RequestBuildError::Io { error })?;
                (encoder.finish().expect("failed to encode"), true)
            }
        };
        Ok(LogApiRequest {
            serialized_payload_bytes_len,
            payload_members_len: total_members,
            api_key: Arc::clone(&api_key),
            is_compressed,
            body: encoded_body,
            finalizers,
        })
    }
}

#[async_trait]
impl<S> StreamSink for LogSink<S>
where
    S: Service<LogApiRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: Into<EventStatus> + Send + 'static,
    S::Error: Debug + Into<crate::Error> + Send,
{
    async fn run(&mut self, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let io_bandwidth = 64;
        let (io_tx, io_rx) = channel(io_bandwidth);
        let service = self
            .service
            .take()
            .expect("same sink should not be run twice");
        let acker = self
            .acker
            .take()
            .expect("same sink should not be run twice");
        let encoding = self
            .encoding
            .take()
            .expect("same sink should not be run twice");
        let default_api_key = Arc::clone(&self.default_api_key);
        let compression = self.compression;
        let log_schema = self.log_schema;

        let io = run_io(io_rx, service, acker).in_current_span();
        let _ = tokio::spawn(io);

        let batcher = Batcher::new(
            input,
            EventPartitioner::default(),
            self.timeout,
            NonZeroUsize::new(MAX_PAYLOAD_ARRAY).unwrap(),
            NonZeroUsize::new(BATCH_GOAL_BYTES),
        )
        .map(|(maybe_key, batch)| {
            let key = maybe_key.unwrap_or_else(|| Arc::clone(&default_api_key));
            let request_builder = RequestBuilder::new(encoding.clone(), compression, log_schema);
            tokio::spawn(async move { request_builder.build(key, batch) })
        })
        .buffer_unordered(io_bandwidth);
        pin!(batcher);

        while let Some(batch_join) = batcher.next().await {
            match batch_join {
                Ok(batch_request) => match batch_request {
                    Ok(request) => {
                        if io_tx.send(request).await.is_err() {
                            error!(
                            "Sink I/O channel should not be closed before sink itself is closed."
                        );
                            return Err(());
                        }
                    }
                    Err(error) => {
                        error!("Sink was unable to construct a payload body: {}", error);
                        return Err(());
                    }
                },
                Err(error) => {
                    error!("Task failed to properly join: {}", error);
                    return Err(());
                }
            }
        }

        Ok(())
    }
}

async fn run_io<S>(mut rx: Receiver<LogApiRequest>, mut service: S, acker: Acker)
where
    S: Service<LogApiRequest>,
    S::Future: Send + 'static,
    S::Response: Into<EventStatus> + Send + 'static,
    S::Error: Debug + Into<crate::Error> + Send,
{
    let in_flight = FuturesUnordered::new();
    let mut pending_acks: HashMap<u64, usize, BuildHasherDefault<XxHash64>> = HashMap::default();
    let mut seq_head: u64 = 0;
    let mut seq_tail: u64 = 0;

    pin!(in_flight);

    loop {
        gauge!("inflight_requests", in_flight.len() as f64);
        select! {
            Some(req) = rx.recv() => {
                // Rebind the variable to avoid a bug with the pattern matching
                // in `select!`: https://github.com/tokio-rs/tokio/issues/4076
                let mut req = req;
                let seqno = seq_head;
                seq_head += 1;

                let (tx, rx) = oneshot::channel();

                in_flight.push(rx);

                trace!(
                    message = "Submitting service request.",
                    in_flight_requests = in_flight.len()
                );
                // TODO: I'm not entirely happy with how we're smuggling
                // batch_size/finalizers this far through, from the finished
                // batch all the way through to the concrete request type...we
                // lifted this code from `ServiceSink` directly, but we should
                // probably treat it like `PartitionBatcher` and shove it into a
                // single, encapsulated type instead.
                let batch_size = req.payload_members_len;
                let finalizers = req.take_finalizers();

                let svc = service.ready().await.expect("should not get error when waiting for svc readiness");
                let fut = svc.call(req)
                    .err_into()
                    .map(move |result| {
                        let status = match result {
                            Err(error) => {
                                error!("Sink IO failed with error: {}", error);
                                EventStatus::Failed
                            },
                            Ok(response) => { response.into() }
                        };
                        finalizers.update_status(status);
                        // If the rx end is dropped we still completed
                        // the request so this is a weird case that we can
                        // ignore for now.
                        let _ = tx.send((seqno, batch_size));
                    })
                    .instrument(info_span!("request", request_id = %seqno));
                tokio::spawn(fut);
            },

            Some(Ok((seqno, batch_size))) = in_flight.next() => {
                trace!("pending batch {} finished (n={})", seqno, batch_size);
                pending_acks.insert(seqno, batch_size);

                let mut num_to_ack = 0;
                while let Some(ack_size) = pending_acks.remove(&seq_tail) {
                    num_to_ack += ack_size;
                    seq_tail += 1
                }
                trace!(message = "Acking events.", acking_num = num_to_ack);
                acker.ack(num_to_ack);
            },

            else => break
        }
    }
}
