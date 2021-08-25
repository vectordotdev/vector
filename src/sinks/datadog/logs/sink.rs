use crate::sinks::datadog::logs::config::Encoding;
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
use itertools::Itertools;
use snafu::Snafu;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::io::Write;
use std::iter::Iterator;
use tokio::time::{self, Duration};
use tower::Service;
use twox_hash::XxHash64;
use vector_core::config::{log_schema, LogSchema};
use vector_core::event::EventStatus;
use vector_core::event::{Event, EventFinalizers, LogEvent, Value};
use vector_core::sink::StreamSink;
use vector_core::ByteSizeOf;

#[derive(Debug, Snafu)]
pub enum BuildError {
    #[snafu(display("The builder is missing the URI to use for Datadog's logs API"))]
    MissingUri,
    #[snafu(display("The builder is missing an HTTP client to use Datadog's logs API"))]
    MissingHttpClient,
}

#[derive(Debug, Snafu)]
pub enum LogApiError {}

#[derive(Debug)]
pub struct LogApiBuilder<Client>
where
    Client: Service<Request<Body>> + Send + Clone + Unpin,
    Client::Future: Send,
    Client::Response: Send,
    Client::Error: Send,
{
    encoding: EncodingConfigWithDefault<Encoding>,
    http_client: Option<Client>,
    datadog_uri: Option<Uri>,
    default_api_key: Option<Box<str>>,
    compression: Option<Compression>,
    timeout: Option<Duration>,
    bytes_stored_limit: u64,
    log_schema_message_key: Option<&'static str>,
    log_schema_timestamp_key: Option<&'static str>,
    log_schema_host_key: Option<&'static str>,
}

impl<Client> Default for LogApiBuilder<Client>
where
    Client: Service<Request<Body>> + Send + Clone + Unpin,
    Client::Future: Send,
    Client::Response: Send,
    Client::Error: Send,
{
    fn default() -> Self {
        Self {
            encoding: Default::default(),
            http_client: None,
            datadog_uri: None,
            default_api_key: None,
            compression: None,
            timeout: None,
            bytes_stored_limit: u64::max_value(),
            log_schema_message_key: None,
            log_schema_timestamp_key: None,
            log_schema_host_key: None,
        }
    }
}

impl<Client> LogApiBuilder<Client>
where
    Client: Service<Request<Body>> + Send + Clone + Unpin,
    Client::Future: Send,
    Client::Response: Send,
    Client::Error: Send,
{
    pub fn log_schema(mut self, log_schema: &'static LogSchema) -> Self {
        self.log_schema_message_key = Some(log_schema.message_key());
        self.log_schema_timestamp_key = Some(log_schema.timestamp_key());
        self.log_schema_host_key = Some(log_schema.host_key());
        self
    }

    pub fn encoding(mut self, encoding: EncodingConfigWithDefault<Encoding>) -> Self {
        self.encoding = encoding;
        self
    }

    pub fn default_api_key(mut self, api_key: Box<str>) -> Self {
        self.default_api_key = Some(api_key);
        self
    }

    pub fn bytes_stored_limit(mut self, limit: u64) -> Self {
        self.bytes_stored_limit = limit;
        self
    }

    // TODO enable and set from config
    // pub fn batch_timeout(mut self, timeout: Duration) -> Self {
    //     self.timeout = Some(timeout);
    //     self
    // }

    pub fn http_client(mut self, client: Client) -> Self {
        self.http_client = Some(client);
        self
    }

    pub fn datadog_uri(mut self, uri: Uri) -> Self {
        self.datadog_uri = Some(uri);
        self
    }

    pub fn compression(mut self, compression: Compression) -> Self {
        self.compression = Some(compression);
        self
    }

    pub fn build(self) -> Result<LogApi<Client>, BuildError> {
        let mut key_slab = HashMap::default();
        let default_api_key = self.default_api_key.unwrap();
        let default_api_key_id = hash(&default_api_key);
        key_slab.insert(default_api_key_id, default_api_key);

        let log_api = LogApi {
            default_api_key: default_api_key_id,
            key_slab,
            event_batches: HashMap::default(),
            bytes_stored_limit: self.bytes_stored_limit as usize,
            bytes_stored: 0,
            compression: self.compression.unwrap_or_default(),
            datadog_uri: self.datadog_uri.ok_or(BuildError::MissingUri)?,
            encoding: self.encoding,
            http_client: self.http_client.ok_or(BuildError::MissingHttpClient)?,
            log_schema_host_key: self.log_schema_host_key.unwrap_or(log_schema().host_key()),
            log_schema_message_key: self
                .log_schema_message_key
                .unwrap_or(log_schema().message_key()),
            log_schema_timestamp_key: self
                .log_schema_timestamp_key
                .unwrap_or(log_schema().timestamp_key()),
            timeout: self.timeout.unwrap_or(Duration::from_secs(60)),
        };
        Ok(log_api)
    }
}

const MAX_PAYLOAD_ARRAY: usize = 1_000;

#[derive(Debug, Default)]
struct FlushMetrics {
    processed_bytes_total: u64,
    processed_events_total: u64,
    success: bool,
}

#[derive(Debug)]
pub struct LogApi<Client>
where
    Client: Service<Request<Body>> + Send + Clone + Unpin,
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

#[derive(serde::Serialize)]
struct Payload {
    members: Vec<BTreeMap<String, Value>>,
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
) -> Request<Body> {
    let total_members = members.len();
    assert!(total_members <= 1_000);
    let payload = Payload { members };
    let body: Vec<u8> = serde_json::to_vec(&payload).expect("failed to encode to json");

    let request = Request::post(datadog_uri)
        .header("Content-Type", "application/json")
        .header("DD-API-KEY", api_key);
    let serialized_payload_len = body.len();
    metrics::histogram!("encoded_payload_size_bytes", serialized_payload_len as f64);
    metrics::gauge!("encoded_payload_size_members", total_members as f64);
    let (request, encoded_body) = match compression {
        Compression::None => (request, body),
        Compression::Gzip(level) => {
            let level = level.unwrap_or(GZIP_FAST);
            let mut encoder = GzEncoder::new(
                Vec::with_capacity(serialized_payload_len),
                flate2::Compression::new(level as u32),
            );

            encoder.write_all(&body).expect("failed to write body");
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
        // .map_err(Into::into)
        .expect("failed to make request")
}

impl<Client> LogApi<Client>
where
    Client: Service<Request<Body>> + Send + Clone + Unpin,
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
            let key_hash = hash(key);
            // TODO it'd be nice to avoid passing through String
            self.key_slab
                .entry(key_hash)
                .or_insert_with(|| String::from(key).into_boxed_str());
            key_hash
        } else {
            self.default_api_key
        }
    }

    /// Stores the `Event` in its batch
    ///
    /// This function stores the `Event` in its `id` appropriate batch. This
    /// will accumulate until a flush is done, triggered by the total byte size
    /// in storage here.
    ///
    /// # Panics
    ///
    /// This function will panic if there is no space in the underlying batch.
    #[inline]
    fn store_event(&mut self, id: u64, event: Event) {
        self.event_batches
            .entry(id)
            .or_insert_with(|| Vec::with_capacity(MAX_PAYLOAD_ARRAY))
            .push(event)
    }

    /// Flush the `Event` batches out to Datadog API
    // TODO make caller pass in a buffer
    async fn flush_to_api<'a>(
        self: &'a mut Self,
    ) -> Result<impl Iterator<Item = impl Future<Output = ()>> + Send + 'a, ()> {
        metrics::counter!("flush", 1);
        // The Datadog API makes no claim on how many requests we can make at
        // one time. We rely on the http client to handle retries and what
        // not. This implementation stacks up futures for each key present in
        // the batches and runs them unordered, returning when the last
        // completes.

        // NOTE I would prefer to map directly over the Drain without going
        // through a vec but mixing ownership confused me greatly, since we need
        // to call back to `self` to flush the batch.
        let batches: Vec<(u64, Vec<Event>)> = self.event_batches.drain().collect();
        poll_fn(|cx| self.http_client.poll_ready(cx))
            .await
            .map_err(|_e| ())?;
        let mut future_buffer = Vec::with_capacity(128);

        for (api_key_id, batch_by_bytes) in batches.into_iter() {
            let api_key = self
                .key_slab
                .get(&api_key_id)
                .expect("impossible situation");
            // Because we track how large a batch is by _bytes_ but the API
            // tracks by _members_ we have to make a conversion here.
            for (members, finalizers) in batch_by_bytes
                .into_iter()
                .chunks(MAX_PAYLOAD_ARRAY)
                .into_iter()
                .map(|batch| dissect_batch(batch.collect()))
            {
                let mut flush_metrics = FlushMetrics::default();
                let request = build_request(
                    members,
                    &api_key[..],
                    self.datadog_uri.clone(),
                    &self.compression,
                    &mut flush_metrics,
                );
                let fut = self.http_client.call(request).map(move |result| {
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
                future_buffer.push(fut);
            }
        }
        Ok(future_buffer.into_iter())
    }
}

#[inline]
fn hash(input: &str) -> u64 {
    let mut hasher = XxHash64::default();
    hasher.write(input.as_bytes());
    hasher.finish()
}

// NOTE likely implementation for #8491
fn rename_key(from_key: &'static str, to_key: &'static str, log: &mut LogEvent) {
    if from_key != to_key {
        if let Some(val) = log.remove(from_key) {
            log.insert_flat(to_key, val);
        }
    }
}

#[async_trait]
impl<Client> StreamSink for LogApi<Client>
where
    Client: Service<Request<Body>> + Send + Clone + Unpin,
    Client::Future: Send,
    Client::Response: Send,
    Client::Error: Send,
{
    async fn run(&mut self, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let encoding = self.encoding.clone();
        let message_key = self.log_schema_message_key;
        let timestamp_key = self.log_schema_timestamp_key;
        let host_key = self.log_schema_host_key;

        let mut input = input.map(|mut event| {
            let log = event.as_mut_log();
            rename_key(message_key, "message", log);
            rename_key(timestamp_key, "date", log);
            rename_key(host_key, "host", log);
            encoding.apply_rules(&mut event);
            event
        });

        let mut interval = time::interval(self.timeout);
        let mut flushes = FuturesUnordered::new();
        metrics::gauge!("bytes_stored_limit", self.bytes_stored_limit as f64);
        loop {
            metrics::gauge!("outstanding_requests", flushes.len() as f64);
            metrics::gauge!("bytes_stored", self.bytes_stored as f64);
            // TODO the current ticking method does not take into account
            // flushing. That is, no matter what, we will always flush every
            // interval even if we just finished flushing.
            tokio::select! {
                biased;

                // TODO better results when we immediately make a request once
                // 1k members are available. Drop the bytes size thing and only
                // track by how large a flush ID is. Change flush_to_api to JUST
                // flush by ID.

                Some(event) = input.next() => {
                    let key_id = self.register_key_id(&event);
                    let event_size = event.size_of();
                    if self.bytes_stored_limit < self.bytes_stored + event_size {
                        // We've gone over the bytes limit and must flush our
                        // batches. As a future optimization, since we haven't
                        // passed the timeout yet, we might consider only
                        // flushing a single large batch. As of self writing the
                        // batch structure doesn't allow sorting by size so we
                        // just ship the whole thing.
                        flushes.extend(self.flush_to_api().await?);
                        self.bytes_stored = 0;
                    }
                    self.store_event(key_id, event);
                    self.bytes_stored += event_size;
                }
                Some(()) = flushes.next() => {
                    // nothing, intentionally
                }
                _ = interval.tick() => {
                    flushes.extend(self.flush_to_api().await?)
                },
            }
        }
    }
}
