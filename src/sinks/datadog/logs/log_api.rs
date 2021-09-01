use crate::sinks::datadog::logs::config::Encoding;
use crate::sinks::datadog::logs::log_api::builder::LogApiBuilder;
use crate::sinks::datadog::logs::log_api::errors::FlushError;
use crate::sinks::util::buffer::GZIP_FAST;
use crate::sinks::util::encoding::{EncodingConfigWithDefault, EncodingConfiguration};
use crate::sinks::util::Compression;
use async_trait::async_trait;
use flate2::write::GzEncoder;
use futures::future::{poll_fn, FutureExt};
use futures::stream::BoxStream;
use futures::Future;
use futures::StreamExt;
use http::response::Response;
use http::{Request, Uri};
use hyper::Body;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::io::Write;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::time::Duration;
use tower::Service;
use vector_core::event::EventStatus;
use vector_core::event::{Event, EventFinalizers, Value};
use vector_core::partition::Partitioner;
use vector_core::sink::StreamSink;
use vector_core::stream::batcher::{Batcher, BatcherTimer};

mod builder;
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
    members: &Vec<BTreeMap<String, Value>>,
    api_key: &str,
    datadog_uri: Uri,
    compression: &Compression,
) -> Result<Request<Body>, FlushError> {
    let total_members = members.len();
    assert!(total_members <= MAX_PAYLOAD_ARRAY);
    let body: Vec<u8> = serde_json::to_vec(members).expect("failed to encode to json");

    let request = Request::post(datadog_uri)
        .header("Content-Type", "application/json")
        .header("DD-API-KEY", api_key);
    let serialized_payload_len = body.len();
    // metrics::histogram!("encoded_payload_size_bytes", serialized_payload_len as f64);
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
    // flush_metrics.processed_bytes_total = serialized_payload_len as u64;
    // flush_metrics.processed_events_total = total_members as u64;
    request
        .header("Content-Length", encoded_body.len())
        .body(Body::from(encoded_body))
        .map_err(Into::into)
}

#[derive(Default)]
struct EventPartitioner {}

impl Partitioner for EventPartitioner {
    type Item = Event;
    type Key = Arc<str>;

    fn partition(&self, item: &Self::Item) -> Option<Self::Key> {
        item.metadata().datadog_api_key().clone()
    }
}

#[derive(Debug)]
pub struct LogApi<Client>
where
    Client: Service<Request<Body>> + Send + Unpin,
    Client::Future: Send,
    Client::Response: Send + Debug,
    Client::Error: Send + Debug,
{
    /// The default Datadog API key to use
    ///
    /// In some instances an `Event` will come in on the stream with an
    /// associated API key. That API key is the one it'll get batched up by but
    /// otherwise we will see `Event` instances with no associated key. In that
    /// case we batch them by this default.
    default_api_key: Arc<str>,
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

fn flush_to_api<'a, Client>(
    http_client: &'a mut Client,
    request: Request<Body>,
    finalizers: Vec<EventFinalizers>,
) -> Result<impl Future<Output = ()> + Send, FlushError>
where
    Client: Service<Request<Body>, Response = http::response::Response<Body>> + Send + Unpin,
    Client::Future: Send,
    Client::Error: Send + Debug,
{
    let fut = http_client.call(request).map(move |result| {
        let status: EventStatus = match result {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    metrics::counter!("flush_success", 1);
                    EventStatus::Delivered
                } else if status.is_server_error() || status.is_client_error() {
                    metrics::counter!("flush_failed", 1);
                    EventStatus::Failed
                } else {
                    unimplemented!()
                }
            }
            Err(_) => {
                metrics::counter!("flush_error", 1);
                EventStatus::Errored
            }
        };
        for finalizer in finalizers {
            finalizer.update_status(status);
        }
    });
    Ok(fut)
}

impl<Client> LogApi<Client>
where
    Client: Service<Request<Body>> + Send + Unpin,
    Client::Future: Send,
    Client::Response: Send + Debug,
    Client::Error: Send + Debug,
{
    pub fn new() -> LogApiBuilder<Client> {
        LogApiBuilder::default().bytes_stored_limit(bytesize::mib(5_u32))
    }
}

#[async_trait]
impl<Client> StreamSink for LogApi<Client>
where
    Client: Service<Request<Body>, Response = Response<Body>> + Send + Sync + Unpin,
    Client::Future: Send,
    Client::Error: Send + Debug,
{
    async fn run(&mut self, input: BoxStream<'_, Event>) -> Result<(), ()> {
        // copy items out of `self`, needed as we don't have ownership
        let compression = self.compression.clone();
        let datadog_uri = self.datadog_uri.clone();
        let default_api_key = self.default_api_key.clone();
        let encoding = self.encoding.clone();
        let host_key = self.log_schema_host_key;
        let message_key = self.log_schema_message_key;
        let timestamp_key = self.log_schema_timestamp_key;

        // Before we start we need to prime the pump on our http client.
        poll_fn(|cx| self.http_client.poll_ready(cx))
            .await
            .map_err(|_e| ())?;

        let input = input.map(|mut event| {
            let log = event.as_mut_log();
            log.rename_key_flat(message_key, "message");
            log.rename_key_flat(timestamp_key, "date");
            log.rename_key_flat(host_key, "host");
            encoding.apply_rules(&mut event);
            event
        });
        let input =
            Batcher::new(
                input,
                EventPartitioner::default(),
                BatcherTimer::new(self.timeout),
                NonZeroUsize::new(MAX_PAYLOAD_ARRAY).unwrap(),
                None,
            )
            .map(|(api_key, events): (Option<Arc<str>>, Vec<Event>)| {
                let api_key = api_key.unwrap_or_else(|| default_api_key.clone());
                let (fields, finalizers) = dissect_batch(events);
                let uri = datadog_uri.clone();
                let request = build_request(&fields, &api_key, uri, &compression);
                (request, finalizers)
            })
            .filter_map(
                |(request, finalizers): (
                    Result<Request<Body>, FlushError>,
                    Vec<EventFinalizers>,
                )| async move {
                    // TODO must address this error somehow
                    if request.is_err() {
                        return None;
                    }
                    Some((request.unwrap(), finalizers))
                },
            );
        let mut input = Box::pin(input);

        while let Some((request, finalizers)) = input.next().await {
            flush_to_api(&mut self.http_client, request, finalizers)
                .expect("unhandled error")
                .await
        }

        Ok(())
    }
}
