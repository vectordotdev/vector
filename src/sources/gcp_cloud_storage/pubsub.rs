use std::{num::NonZeroUsize, panic, sync::Arc, sync::atomic::{AtomicBool, Ordering}};

use bytes::Bytes;
use chrono::{DateTime, Utc};
use http::{Request, StatusCode};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use snafu::Snafu;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use futures::StreamExt;
use tracing::Instrument;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use vector_lib::configurable::configurable_component;
use vector_lib::internal_event::{BytesReceived, Protocol, Registered, InternalEventHandle as _};
use vector_lib::{ByteSizeOf, finalizer::UnorderedFinalizer};
use crate::internal_events::EventsReceived;

use crate::codecs::Decoder;
use crate::{
    config::{SourceAcknowledgementsConfig, SourceContext},
    event::{BatchNotifier, BatchStatus},
    gcp::{GcpAuthenticator, PUBSUB_URL},
    http::HttpClient,
    line_agg,
    shutdown::ShutdownSignal,
    SourceSender,
};
use vector_lib::config::{log_schema, LegacyKey, LogNamespace};
use vector_lib::lookup::{metadata_path, path, PathPrefix};

// Default GCS Storage API base URL
#[allow(dead_code)]
const GCS_BASE_URL: &str = "https://storage.googleapis.com";

// We use a bounded channel for the acknowledgement ID communication
// between the request stream and receiver (similar to main pubsub source)
const ACK_QUEUE_SIZE: usize = 8;

enum PollState {
    RetryNow,
    RetryDelay,
    Shutdown,
}

// prost emits some generated code that includes clones on `Arc`
// objects, which causes a clippy ding on this block. We don't
// directly control the generated code, so allow this lint here.
#[allow(clippy::clone_on_ref_ptr)]
// https://github.com/hyperium/tonic/issues/1350
#[allow(clippy::missing_const_for_fn)]
#[allow(warnings)]
mod proto {
    include!(concat!(env!("OUT_DIR"), "/google.pubsub.v1.rs"));

    use vector_lib::ByteSizeOf;

    impl ByteSizeOf for StreamingPullResponse {
        fn allocated_bytes(&self) -> usize {
            self.received_messages.size_of()
        }
    }

    impl ByteSizeOf for ReceivedMessage {
        fn allocated_bytes(&self) -> usize {
            self.ack_id.size_of() + self.message.as_ref().map_or(0, ByteSizeOf::size_of)
        }
    }

    impl ByteSizeOf for PubsubMessage {
        fn allocated_bytes(&self) -> usize {
            self.data.len()
                + self.message_id.len()
                + self.ordering_key.len()
                + self.attributes.size_of()
        }
    }
}


/// Pub/Sub configuration options for GCS source.
#[serde_as]
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// The Pub/Sub subscription name that receives GCS bucket notifications.
    #[configurable(metadata(
        docs::examples = "projects/my-project/subscriptions/gcs-notifications"
    ))]
    pub subscription: String,

    /// How long to wait while polling the subscription for new messages, in seconds.
    #[serde(default = "default_poll_secs")]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    pub poll_secs: u32,

    /// The maximum number of messages to receive in one pull operation.
    #[serde(default = "default_max_messages")]
    #[configurable(metadata(docs::examples = 100))]
    pub max_messages: u32,

    /// Number of concurrent tasks to create for processing messages.
    #[configurable(metadata(docs::type_unit = "tasks"))]
    #[configurable(metadata(docs::examples = 10))]
    pub concurrency: Option<NonZeroUsize>,

    /// Whether to delete processed messages from the subscription.
    #[serde(default = "default_true")]
    pub delete_message: bool,

    /// Whether to delete messages that failed processing.
    #[serde(default = "default_true")]
    pub delete_failed_message: bool,
}

const fn default_poll_secs() -> u32 {
    15
}

const fn default_max_messages() -> u32 {
    100
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Snafu)]
pub enum IngestorNewError {
    #[snafu(display("Invalid concurrency value"))]
    InvalidConcurrency,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Snafu)]
pub enum ProcessingError {
    #[snafu(display(
        "Could not parse Pub/Sub message as GCS notification: {}",
        source
    ))]
    InvalidMessage { source: serde_json::Error },

    #[snafu(display("Failed to download gs://{}/{}: {}", bucket, object, source))]
    DownloadObject {
        source: Box<dyn std::error::Error + Send + Sync>,
        bucket: String,
        object: String,
    },

    #[snafu(display("Failed to read gs://{}/{}: {}", bucket, object, source))]
    ReadObject {
        source: Box<dyn std::error::Error + Send + Sync>,
        bucket: String,
        object: String,
    },

    #[snafu(display("Failed to send events for gs://{}/{}: {}", bucket, object, source))]
    PipelineSend {
        source: crate::source_sender::ClosedError,
        bucket: String,
        object: String,
    },

    #[snafu(display(
        "Sink reported an error for gs://{}/{} in project {}",
        bucket,
        object,
        project
    ))]
    ErrorAcknowledgement {
        project: String,
        bucket: String,
        object: String,
    },
}

pub struct State {
    project: String,
    auth: GcpAuthenticator,
    http_client: HttpClient,
    subscription: String,
    poll_secs: u32,
    max_messages: u32,
    concurrency: usize,
    delete_message: bool,
    delete_failed_message: bool,
    compression: super::Compression,
    multiline: Option<line_agg::Config>,
    decoder: Decoder,
}

pub struct Ingestor {
    state: Arc<State>,
}

impl Ingestor {
    pub async fn new(
        project: String,
        auth: GcpAuthenticator,
        http_client: HttpClient,
        config: Config,
        compression: super::Compression,
        multiline: Option<line_agg::Config>,
        decoder: Decoder,
    ) -> Result<Ingestor, IngestorNewError> {
        let state = Arc::new(State {
            project,
            auth,
            http_client,
            subscription: config.subscription,
            poll_secs: config.poll_secs,
            max_messages: config.max_messages,
            concurrency: config
                .concurrency
                .map(|n| n.get())
                .unwrap_or_else(crate::num_threads),
            delete_message: config.delete_message,
            delete_failed_message: config.delete_failed_message,
            compression,
            multiline,
            decoder,
        });

        Ok(Ingestor { state })
    }

    pub async fn run(
        self,
        cx: SourceContext,
        acknowledgements: SourceAcknowledgementsConfig,
        log_namespace: LogNamespace,
    ) -> Result<(), ()> {
        let acknowledgements = cx.do_acknowledgements(acknowledgements);
        let mut handles = Vec::new();
        for _ in 0..self.state.concurrency {
            let process = IngestorProcess::new(
                Arc::clone(&self.state),
                cx.out.clone(),
                cx.shutdown.clone(),
                log_namespace,
                acknowledgements,
            );
            let fut = process.run();
            let handle = tokio::spawn(fut.in_current_span());
            handles.push(handle);
        }

        // Wait for all processes to finish
        for handle in handles.drain(..) {
            if let Err(e) = handle.await {
                if e.is_panic() {
                    panic::resume_unwind(e.into_panic());
                }
            }
        }

        Ok(())
    }
}

pub struct IngestorProcess {
    state: Arc<State>,
    out: SourceSender,
    shutdown: ShutdownSignal,
    acknowledgements: bool,
    #[allow(dead_code)]
    log_namespace: LogNamespace,
    bytes_received: Registered<BytesReceived>,
    #[allow(dead_code)]
    events_received: Registered<EventsReceived>,
    retry_delay: std::time::Duration,
}

impl IngestorProcess {
    pub fn new(
        state: Arc<State>,
        out: SourceSender,
        shutdown: ShutdownSignal,
        log_namespace: LogNamespace,
        acknowledgements: bool,
    ) -> Self {
        Self {
            state,
            out,
            shutdown,
            acknowledgements,
            log_namespace,
            bytes_received: register!(BytesReceived::from(Protocol::HTTP)),
            events_received: register!(EventsReceived),
            retry_delay: std::time::Duration::from_secs(1), // Similar to main pubsub source
        }
    }

    async fn run(mut self) {
        let busy_flag = Arc::new(AtomicBool::new(false));

        loop {
            match self.run_once(&busy_flag).await {
                PollState::RetryNow => debug!("Retrying immediately."),
                PollState::RetryDelay => {
                    info!(
                        timeout_secs = self.retry_delay.as_secs_f64(),
                        "Retrying after timeout."
                    );
                    tokio::time::sleep(self.retry_delay).await;
                }
                PollState::Shutdown => break,
            }
        }
    }

    async fn run_once(&mut self, busy_flag: &Arc<AtomicBool>) -> PollState {
        use http::uri::Uri;
        use tonic::transport::{Endpoint, ClientTlsConfig};
        use tonic::{Request, metadata::MetadataValue, Code, Status};

        // Create endpoint (same as original)
        let mut uri: Uri = PUBSUB_URL.parse().unwrap();
        self.state.auth.apply_uri(&mut uri);

        let host = uri.host().unwrap_or("pubsub.googleapis.com");
        let tls_config = ClientTlsConfig::new().domain_name(host);
        let endpoint: Endpoint = uri.to_string().parse().unwrap();
        let endpoint = endpoint.tls_config(tls_config).unwrap();

        let connection = match endpoint.connect().await {
            Ok(connection) => connection,
            Err(error) => {
                error!(message = "Failed to connect to PubSub", %error);
                return PollState::RetryDelay;
            }
        };

        let mut client = proto::subscriber_client::SubscriberClient::with_interceptor(
            connection,
            |mut req: Request<()>| {
                if let Some(token) = self.state.auth.make_token() {
                    let authorization = MetadataValue::try_from(&token).map_err(|_| {
                        Status::new(
                            Code::FailedPrecondition,
                            "Invalid token text returned by GCP",
                        )
                    })?;
                    req.metadata_mut().insert("authorization", authorization);
                }
                Ok(req)
            },
        )
        .max_decoding_message_size(usize::MAX);

        let (ack_ids_sender, ack_ids_receiver) = mpsc::channel(ACK_QUEUE_SIZE);

        // Handle shutdown during startup
        let request_stream = self.request_stream(ack_ids_receiver);
        debug!("Starting streaming pull.");
        let stream = tokio::select! {
            _ = &mut self.shutdown => return PollState::Shutdown,
            result = client.streaming_pull(request_stream) => match result {
                Ok(stream) => stream,
                Err(error) => {
                    error!(message = "Failed to start streaming pull", %error);
                    return PollState::RetryDelay;
                }
            }
        };
        let mut stream = stream.into_inner();

        let (finalizer, mut ack_stream) =
            UnorderedFinalizer::maybe_new(self.acknowledgements, Some(self.shutdown.clone()));
        let mut pending_acks = 0;

        loop {
            tokio::select! {
                biased;
                receipts = futures::StreamExt::next(&mut ack_stream) => if let Some((status, receipts)) = receipts {
                    pending_acks -= 1;
                    if status == BatchStatus::Delivered {
                        ack_ids_sender
                            .send(receipts)
                            .await
                            .unwrap_or_else(|_| unreachable!("request stream never closes"));
                    }
                },
                response = futures::StreamExt::next(&mut stream) => match response {
                    Some(Ok(response)) => {
                        self.handle_response(
                            response,
                            &finalizer,
                            &ack_ids_sender,
                            &mut pending_acks,
                            busy_flag,
                        ).await;
                    }
                    Some(Err(error)) => break self.translate_error(error),
                    None => break PollState::RetryNow,
                },
                _ = &mut self.shutdown, if pending_acks == 0 => return PollState::Shutdown,
                _ = tokio::time::sleep(std::time::Duration::from_secs(self.state.poll_secs as u64)) => {
                    if pending_acks == 0 {
                        // Mark this stream as idle and break if no messages
                        busy_flag.store(false, Ordering::Relaxed);
                        break PollState::RetryDelay;
                    }
                    // Send keepalive
                    ack_ids_sender
                        .send(Vec::new())
                        .await
                        .unwrap_or_else(|_| unreachable!("request stream never closes"));
                }
            }
        }
    }

    fn request_stream(
        &self,
        ack_ids: mpsc::Receiver<Vec<String>>,
    ) -> impl futures::Stream<Item = proto::StreamingPullRequest> + 'static {

        let subscription = self.state.subscription.clone();
        let stream_ack_deadline_seconds = 600; // Default ack deadline
        let ack_ids = ReceiverStream::new(ack_ids).ready_chunks(ACK_QUEUE_SIZE);

        futures_util::StreamExt::chain(
            futures::stream::once(async move {
                // Initial request with subscription info
                proto::StreamingPullRequest {
                    subscription,
                    stream_ack_deadline_seconds,
                    ..Default::default()
                }
            }),
            tokio_stream::StreamExt::map(ack_ids, |chunks| {
                // Subsequent requests only contain ack IDs
                proto::StreamingPullRequest {
                    ack_ids: chunks.into_iter().flatten().collect(),
                    ..Default::default()
                }
            })
        )
    }

    async fn handle_response(
        &mut self,
        response: proto::StreamingPullResponse,
        finalizer: &Option<UnorderedFinalizer<Vec<String>>>,
        ack_ids: &mpsc::Sender<Vec<String>>,
        pending_acks: &mut usize,
        busy_flag: &Arc<AtomicBool>,
    ) {
        if response.received_messages.len() >= 10 { // Consider busy if many messages
            busy_flag.store(true, Ordering::Relaxed);
        }
        self.bytes_received.emit(vector_lib::internal_event::ByteSize(response.size_of()));

        let (batch, notifier) = BatchNotifier::maybe_new_with_receiver(self.acknowledgements);
        let (events, ids) = self.parse_messages(response.received_messages, batch).await;

        let count = events.len();
        match self.out.send_batch(events).await {
            Err(_) => {
                error!(message = "Output channel closed", count);
            }
            Ok(()) => match notifier {
                None => ack_ids
                    .send(ids)
                    .await
                    .unwrap_or_else(|_| unreachable!("request stream never closes")),
                Some(notifier) => {
                    finalizer
                        .as_ref()
                        .expect("Finalizer must have been set up for acknowledgements")
                        .add(ids, notifier);
                    *pending_acks += 1;
                }
            },
        }
    }

    async fn parse_messages(
        &self,
        response: Vec<proto::ReceivedMessage>,
        batch: Option<BatchNotifier>,
    ) -> (Vec<crate::event::Event>, Vec<String>) {
        let mut ack_ids = Vec::with_capacity(response.len());
        let mut events = Vec::new();

        for received in response {
            ack_ids.push(received.ack_id);
            if let Some(message) = received.message {
                // Parse message data as GCS notification
                match serde_json::from_slice::<GcsNotification>(&message.data) {
                    Ok(notification) => {
                        match self.handle_gcs_notification_sync(notification, &batch).await {
                            Ok(event_batch) => {
                                events.extend(event_batch);
                            }
                            Err(error) => {
                                error!(message = "Failed to process GCS notification", %error);
                            }
                        }
                    }
                    Err(error) => {
                        let s = String::from_utf8_lossy(&message.data);
                        error!(message = "Failed to parse PubSub message as GCS notification", %error, data=%s);
                    }
                }
            }
        }
        (events, ack_ids)
    }

    fn translate_error(&self, error: tonic::Status) -> PollState {
        error!(message = "Streaming pull error", %error);
        PollState::RetryDelay
    }

    async fn handle_gcs_notification_sync(
        &self,
        notification: GcsNotification,
        batch: &Option<BatchNotifier>,
    ) -> Result<Vec<crate::event::Event>, ProcessingError> {
        debug!(
            message = "Processing GCS notification",
            bucket = notification.bucket,
            object = notification.name,
        );

        // Download the object from GCS
        let (object_content, content_encoding, content_type) = self.download_object_sync(&notification.bucket, &notification.name).await?;

        // Get object metadata for timestamps
        let timestamp = notification.time_created.clone().and_then(|ts| {
            DateTime::parse_from_rfc3339(&ts)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        });

        // Decompress the object content
        let object_content_len = object_content.len();
        self.bytes_received.emit(vector_lib::internal_event::ByteSize(object_content_len));

        let mut object_reader = super::gcs_object_decoder(
            self.state.compression,
            &notification.name,
            content_encoding.as_deref(),
            content_type.as_deref(),
            object_content,
        )
        .await;

        // Use util::decode_message to parse the content, similar to gcp_pubsub
        let mut object_bytes = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut object_reader, &mut object_bytes)
            .await
            .map_err(|e| ProcessingError::ReadObject {
                source: Box::new(e),
                bucket: notification.bucket.clone(),
                object: notification.name.clone(),
            })?;

        let events: Vec<crate::event::Event> = crate::sources::util::decode_message(
            self.state.decoder.clone(),
            crate::sources::gcp_cloud_storage::GcpCloudStorageConfig::NAME,
            &object_bytes,
            timestamp,
            batch,
            self.log_namespace,
            &self.events_received,
        )
        .map(|mut event| {
            // Add GCS-specific metadata to each event
            self.add_gcs_metadata(&mut event, &notification, timestamp, batch);
            event
        })
        .collect();

        info!(
            message = "Successfully processed GCS object",
            bucket = notification.bucket,
            object = notification.name,
            events_processed = events.len(),
            bytes_processed = object_content_len,
        );

        Ok(events)
    }

    async fn download_object_sync(&self, bucket: &str, object: &str) -> Result<(Bytes, Option<String>, Option<String>), ProcessingError> {
        let url = format!("{}/storage/v1/b/{}/o/{}?alt=media", GCS_BASE_URL, bucket, utf8_percent_encode(object, NON_ALPHANUMERIC));

        let mut request = Request::get(&url)
            .body(hyper::Body::empty())
            .map_err(|e| ProcessingError::DownloadObject {
                source: Box::new(e),
                bucket: bucket.to_string(),
                object: object.to_string(),
            })?;

        // Apply GCP authentication
        self.state.auth.apply(&mut request);

        let response = self.state.http_client
            .send(request)
            .await
            .map_err(|e| ProcessingError::DownloadObject {
                source: Box::new(e),
                bucket: bucket.to_string(),
                object: object.to_string(),
            })?;

        if response.status() != StatusCode::OK {
            return Err(ProcessingError::DownloadObject {
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("HTTP {}", response.status()),
                )),
                bucket: bucket.to_string(),
                object: object.to_string(),
            });
        }

        // Extract Content-Encoding and Content-Type headers before consuming the response
        let content_encoding = response.headers()
            .get("content-encoding")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let content_type = response.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let body = hyper::body::to_bytes(response.into_body())
            .await
            .map_err(|e| ProcessingError::DownloadObject {
                source: Box::new(e),
                bucket: bucket.to_string(),
                object: object.to_string(),
            })?;

        Ok((body, content_encoding, content_type))
    }

    fn add_gcs_metadata(
        &self,
        event: &mut crate::event::Event,
        notification: &GcsNotification,
        timestamp: Option<DateTime<Utc>>,
        _batch: &Option<BatchNotifier>,
    ) {
        if let crate::event::Event::Log(ref mut log) = event {
            // Add GCS-specific metadata
            self.log_namespace.insert_source_metadata(
                crate::sources::gcp_cloud_storage::GcpCloudStorageConfig::NAME,
                log,
                Some(LegacyKey::Overwrite(path!("bucket"))),
                path!("bucket"),
                Bytes::from(notification.bucket.as_bytes().to_vec()),
            );

            self.log_namespace.insert_source_metadata(
                crate::sources::gcp_cloud_storage::GcpCloudStorageConfig::NAME,
                log,
                Some(LegacyKey::Overwrite(path!("object"))),
                path!("object"),
                Bytes::from(notification.name.as_bytes().to_vec()),
            );

            if let Some(ref generation) = notification.generation {
                self.log_namespace.insert_source_metadata(
                    crate::sources::gcp_cloud_storage::GcpCloudStorageConfig::NAME,
                    log,
                    Some(LegacyKey::Overwrite(path!("generation"))),
                    path!("generation"),
                    Bytes::from(generation.as_bytes().to_vec()),
                );
            }

            // Set source type
            self.log_namespace.insert_vector_metadata(
                log,
                log_schema().source_type_key(),
                path!("source_type"),
                Bytes::from_static(crate::sources::gcp_cloud_storage::GcpCloudStorageConfig::NAME.as_bytes()),
            );

            // Handle timestamp
            match self.log_namespace {
                LogNamespace::Vector => {
                    if let Some(timestamp) = timestamp {
                        log.insert(metadata_path!(crate::sources::gcp_cloud_storage::GcpCloudStorageConfig::NAME, "timestamp"), timestamp);
                    }
                    log.insert(metadata_path!("vector", "ingest_timestamp"), Utc::now());
                }
                LogNamespace::Legacy => {
                    if let Some(timestamp_key) = log_schema().timestamp_key() {
                        log.try_insert(
                            (PathPrefix::Event, timestamp_key),
                            timestamp.unwrap_or_else(Utc::now),
                        );
                    }
                }
            };

            // Batch notifier is handled by util::decode_message
        }
    }
}


/// GCS bucket notification message structure
/// Based on: https://cloud.google.com/storage/docs/pubsub-notifications#payload
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GcsNotification {
    /// The bucket name
    pub bucket: String,
    /// The object name
    pub name: String,
    /// Object generation number
    pub generation: Option<String>,
    /// Object metageneration number
    pub metageneration: Option<String>,
    /// Time when the object was created (RFC 3339 format)
    pub time_created: Option<String>,
    /// Time when the object was updated (RFC 3339 format)
    pub updated: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gcs_notification_parsing() {
        let json = r#"{
            "bucket": "my-bucket",
            "name": "path/to/file.log",
            "eventType": "OBJECT_FINALIZE",
            "generation": "1234567890",
            "timeCreated": "2023-10-01T12:00:00.000Z"
        }"#;

        let notification: GcsNotification = serde_json::from_str(json).unwrap();
        assert_eq!(notification.bucket, "my-bucket");
        assert_eq!(notification.name, "path/to/file.log");
    }
}
