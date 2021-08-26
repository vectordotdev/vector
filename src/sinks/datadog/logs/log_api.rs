use crate::sinks::datadog::logs::config::Encoding;
use crate::sinks::datadog::logs::log_api::builder::LogApiBuilder;
use crate::sinks::datadog::logs::log_api::errors::FlushError;
use crate::sinks::util::buffer::GZIP_FAST;
use crate::sinks::util::encoding::{EncodingConfigWithDefault, EncodingConfiguration};
use crate::sinks::util::Compression;
use async_trait::async_trait;
use flate2::write::GzEncoder;
use futures::future::{poll_fn, FutureExt};
use futures::stream::{BoxStream, FuturesUnordered};
use futures::Future;
use futures::StreamExt;
use http::{Request, Uri};
use hyper::Body;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use std::io::Write;
use tokio::time::{self, Duration};
use tower::Service;
use twox_hash::XxHash64;
use vector_core::event::EventStatus;
use vector_core::event::{Event, EventFinalizers, Value};
use vector_core::sink::StreamSink;
use vector_core::ByteSizeOf;

mod builder;
mod common;
mod errors;

const MAX_PAYLOAD_ARRAY: usize = 1_000;

#[derive(Debug, Default)]
struct FlushMetrics {
    processed_bytes_total: u64,
    processed_events_total: u64,
    success: bool,
}

#[inline]
fn dissect_batch(batch: Vec<Event>) -> (Vec<BTreeMap<String, Value>>, Vec<EventFinalizers>) {
    let mut members: Vec<BTreeMap<String, Value>> = Vec::with_capacity(batch.len());
    let mut finalizers: Vec<EventFinalizers> = Vec::with_capacity(batch.len());
    for event in batch.into_iter() {
        let (fields, mut metadata) = event.into_log().into_parts();
        members.push(fields);
        finalizers.push(metadata.take_finalizers());
    }
    (members, finalizers)
}

fn build_request(
    members: Vec<BTreeMap<String, Value>>,
    api_key: &str,
    datadog_uri: Uri,
    compression: &Compression,
    flush_metrics: &mut FlushMetrics,
) -> Result<Request<Body>, FlushError> {
    let total_members = members.len();
    assert!(total_members <= 1_000);
    let body: Vec<u8> = serde_json::to_vec(&members).expect("failed to encode to json");

    let request = Request::post(datadog_uri)
        .header("Content-Type", "application/json")
        .header("DD-API-KEY", api_key);
    let serialized_payload_len = body.len();
    metrics::histogram!("encoded_payload_size_bytes", serialized_payload_len as f64);
    let (request, encoded_body) = match compression {
        Compression::None => (request, body),
        Compression::Gzip(level) => {
            let level = level.unwrap_or(GZIP_FAST);
            let mut encoder = GzEncoder::new(
                Vec::with_capacity(serialized_payload_len),
                flate2::Compression::new(level as u32),
            );

            encoder.write_all(&body)?;
            (
                request.header("Content-Encoding", "gzip"),
                encoder.finish().expect("failed to encode"),
            )
        }
    };
    flush_metrics.processed_bytes_total = serialized_payload_len as u64;
    flush_metrics.processed_events_total = total_members as u64;
    request
        .header("Content-Length", encoded_body.len())
        .body(Body::from(encoded_body))
        .map_err(Into::into)
}

#[derive(Debug)]
pub struct LogApi<Client>
where
    Client: Service<Request<Body>> + Send + Unpin,
    Client::Future: Send,
    Client::Response: Send,
    Client::Error: Send,
{
    /// The default Datadog API key to use
    ///
    /// In some instances an `Event` will come in on the stream with an
    /// associated API key. That API key is the one it'll get batched up by but
    /// otherwise we will see `Event` instances with no associated key. In that
    /// case we batch them by this default.
    ///
    /// Note that this is a `usize` and not a `Box<str>` or similar. This sink
    /// stores all API keys in a slab and only materializes the actual API key
    /// when needed.
    default_api_key: u64,
    /// The slab of API keys
    ///
    /// This slab holds the actual materialized API key in the form of a
    /// `Box<str>`. This avoids having lots of little strings running around
    /// with the downside of being an unbounded structure, in the present
    /// implementation.
    key_slab: HashMap<u64, Box<str>, BuildHasherDefault<XxHash64>>,
    /// The batches of `Event` instances, sorted by API key
    event_batches: HashMap<u64, Vec<Event>, BuildHasherDefault<XxHash64>>,
    /// The total duration before a flush is forced
    ///
    /// This value sets the duration that is allowed to ellapse prior to a flush
    /// of all buffered `Event` instances.
    timeout: Duration,
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
    /// The "message" key for the global log schema
    log_schema_message_key: &'static str,
    /// The "timestamp" key for the global log schema
    log_schema_timestamp_key: &'static str,
    /// The "host" key for the global log schema
    log_schema_host_key: &'static str,
    /// The encoding of payloads
    ///
    /// This struct always generates JSON payloads. However we do, technically,
    /// allow the user to set the encoding to a single value -- JSON -- and this
    /// encoding comes with rules on sanitizing the payload which must be
    /// applied.
    encoding: EncodingConfigWithDefault<Encoding>,
    /// The API http client
    http_client: Client,
    /// The URI of the Datadog API
    datadog_uri: Uri,
    /// The compression technique to use when sending requests to the Datadog
    /// API
    compression: Compression,
}

impl<Client> LogApi<Client>
where
    Client: Service<Request<Body>> + Send + Unpin,
    Client::Future: Send,
    Client::Response: Send,
    Client::Error: Send,
{
    pub fn new() -> LogApiBuilder<Client> {
        LogApiBuilder::default().bytes_stored_limit(bytesize::mib(5_u32))
    }

    /// Calculates the API key ID of an `Event`
    ///
    /// This function calculates the API key ID of a given `Event`. As a
    /// side-effect it mutates internal state of the struct allowing callers to
    /// use the ID to retrieve a `Box<str>` of the key at a later time.
    fn register_key_id(&mut self, event: &Event) -> u64 {
        if let Some(api_key) = event.metadata().datadog_api_key() {
            let key = api_key.as_ref();
            let key_hash = common::hash(key);
            // TODO it'd be nice to avoid passing through String
            self.key_slab
                .entry(key_hash)
                .or_insert_with(|| String::from(key).into_boxed_str());
            key_hash
        } else {
            self.default_api_key
        }
    }

    /// Determines if there is space in the batch for an additional `Event`
    ///
    /// This function determines whether adding a new `Event` to the batch
    /// associated with `id` will take that batch over the maximum payload
    /// size. If this function returns true the user may call [`store_event`]
    /// else they must clear out space in the batch or drop whatever `Event`
    /// they're holding.
    #[inline]
    fn has_space(&self, id: u64) -> bool {
        if let Some(arr) = self.event_batches.get(&id) {
            arr.len() < MAX_PAYLOAD_ARRAY
        } else {
            true
        }
    }

    /// Stores the `Event` in its batch
    ///
    /// This function stores the `Event` in its `id` appropriate batch. Caller
    /// MUST confirm that there is space in the batch for the `Event` by calling
    /// `has_space`, which must return true.
    ///
    /// # Panics
    ///
    /// This function will panic if there is no space in the underlying batch.
    #[inline]
    fn store_event(&mut self, id: u64, event: Event) {
        let arr = self
            .event_batches
            .entry(id)
            .or_insert_with(|| Vec::with_capacity(MAX_PAYLOAD_ARRAY));
        assert!(arr.len() + 1 <= MAX_PAYLOAD_ARRAY);
        arr.push(event);
    }

    /// Flush the `Event` batches out to Datadog API
    ///
    /// When this function is called we are guaranteed to have less than
    /// MAX_PAYLOAD_ARRAY total elements in a batch, whether this is triggered
    /// by timeout or noting that `has_space` failes.
    ///
    /// # Panics
    ///
    /// This function will panic if the batch size exceeds
    /// MAX_PAYLOAD_ARRAY. Doing so implies a serious bug in the logic of this
    /// implementaiton.
    fn flush_to_api<'a>(
        self: &'a mut Self,
        api_key_id: u64,
        batch: Vec<Event>,
    ) -> Result<impl Future<Output = ()> + Send, FlushError> {
        assert!(batch.len() <= MAX_PAYLOAD_ARRAY);

        let api_key = self
            .key_slab
            .get(&api_key_id)
            .expect("impossible situation");

        let (members, finalizers) = dissect_batch(batch);
        let mut flush_metrics = FlushMetrics::default();
        let request = build_request(
            members,
            &api_key[..],
            self.datadog_uri.clone(),
            &self.compression,
            &mut flush_metrics,
        );
        let fut = self.http_client.call(request?).map(move |result| {
            let status: EventStatus = match result {
                Ok(_) => {
                    metrics::counter!("flush_success", 1);
                    EventStatus::Delivered
                }
                Err(_) => {
                    metrics::counter!("flush_error", 1);
                    EventStatus::Errored
                }
            };
            for finalizer in finalizers {
                finalizer.update_status(status);
            }
            metrics::counter!("processed_bytes_total", flush_metrics.processed_bytes_total);
            metrics::counter!(
                "processed_events_total",
                flush_metrics.processed_events_total
            );
            ()
        });
        Ok(fut)
    }
}

#[async_trait]
impl<Client> StreamSink for LogApi<Client>
where
    Client: Service<Request<Body>> + Send + Unpin,
    Client::Future: Send,
    Client::Response: Send,
    Client::Error: Send,
{
    async fn run(&mut self, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let encoding = self.encoding.clone();
        let message_key = self.log_schema_message_key;
        let timestamp_key = self.log_schema_timestamp_key;
        let host_key = self.log_schema_host_key;

        // Before we start we need to prime the pump on our http client.
        poll_fn(|cx| self.http_client.poll_ready(cx))
            .await
            .map_err(|_e| ())?;

        let mut input = input.map(|mut event| {
            let log = event.as_mut_log();
            log.rename_key_flat(message_key, "message");
            log.rename_key_flat(timestamp_key, "date");
            log.rename_key_flat(host_key, "host");
            encoding.apply_rules(&mut event);
            event
        });

        let mut interval = time::interval(self.timeout);
        let mut flushes = FuturesUnordered::new();
        loop {
            tokio::select! {
                biased;
                Some(()) = flushes.next() => {
                    // nothing, intentionally
                }
                Some(event) = input.next() => {
                    let key_id = self.register_key_id(&event);
                    let event_size = event.size_of();
                    if !self.has_space(key_id) || self.bytes_stored_limit < self.bytes_stored + event_size {
                        let batch = self
                            .event_batches
                            .remove(&key_id)
                            .expect("impossible situation");
                        // TODO currently we do serialization inline to this
                        // call, which is time consuming relative to processing
                        // incoming events. If we could serialize in parallel to
                        // doing IO we'd end up ahead most likely.
                        let flush = self.flush_to_api(key_id, batch).expect("unsure how to handle error");
                        flushes.push(flush);
                        self.bytes_stored = 0; // TODO logic is now wrong since we flush single-batch
                    }
                    self.store_event(key_id, event);
                    self.bytes_stored += event_size;
                }
                _ = interval.tick() => {
                    // The current ticking method does not take into account
                    // flushing. That is, no matter what, we will always flush
                    // every interval even if we just finished flushing. We
                    // could avoid this if we were able to keep track of when a
                    // batch was last flushed _or_ if we pull batching
                    // responsibility (+ timeouts) into a stream manipulator
                    // prior to this sink.
                    let keys: Vec<u64> = self.event_batches.keys().into_iter().map(|x| *x).collect();
                    for key_id in keys.into_iter() {
                        let batch = self
                            .event_batches
                            .remove(&key_id)
                            .expect("impossible situation");
                        let flush = self.flush_to_api(key_id, batch).expect("unsure how to handle error");
                        flushes.push(flush);
                    }
                    self.bytes_stored = 0;
                },
            }
        }
    }
}
