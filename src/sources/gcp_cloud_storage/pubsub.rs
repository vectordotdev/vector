use std::{
    future::ready,
    num::NonZeroUsize,
    panic,
    sync::Arc,
    time::{Duration, Instant},
};

use bytes::Bytes;
use chrono::Utc;
use futures::{FutureExt, Stream, StreamExt};
use serde::Deserialize;
use smallvec::SmallVec;
use snafu::Snafu;
use tokio::{pin, select};
use tokio_util::codec::FramedRead;
use tracing::Instrument;
use vector_lib::{
    codecs::decoding::FramingError,
    config::{LegacyKey, LogNamespace, log_schema},
    configurable::configurable_component,
    event::MaybeAsLogMut,
    internal_event::{
        ByteSize, BytesReceived, CountByteSize, InternalEventHandle as _, Protocol, Registered,
    },
    lookup::{PathPrefix, metadata_path, path},
    source_sender::SendError,
};

use crate::{
    SourceSender,
    codecs::Decoder,
    common::backoff::ExponentialBackoff,
    config::{SourceAcknowledgementsConfig, SourceContext},
    event::{BatchNotifier, BatchStatus, EstimatedJsonEncodedSizeOf, Event, LogEvent},
    gcp::GcpAuthenticator,
    http::HttpClient,
    internal_events::{
        EventsReceived, GcsNotificationInvalidEventIgnored, GcsObjectProcessingFailed,
        GcsObjectProcessingSucceeded, GcsPubsubMessageAcknowledgeError,
        GcsPubsubMessageAcknowledgeSucceeded, GcsPubsubMessageProcessingError,
        GcsPubsubMessageProcessingSucceeded, GcsPubsubMessageReceiveError,
        GcsPubsubMessageReceiveSucceeded, StreamClosedError,
    },
    line_agg::{self, LineAgg},
    shutdown::ShutdownSignal,
    sources::gcp_cloud_storage::GcsSourceConfig,
};

/// Pub/Sub subscription configuration for GCS notifications.
#[configurable_component]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct PubsubConfig {
    /// The Pub/Sub subscription name to poll for GCS notifications.
    #[configurable(metadata(docs::examples = "my-gcs-notifications-sub"))]
    pub subscription: String,

    /// The Pub/Sub endpoint to use.
    ///
    /// This can be used to point to a Pub/Sub emulator.
    #[serde(default = "default_pubsub_endpoint")]
    #[derivative(Default(value = "default_pubsub_endpoint()"))]
    #[configurable(metadata(docs::examples = "https://pubsub.googleapis.com"))]
    pub endpoint: String,

    /// How long to wait while polling for new messages, in seconds.
    #[serde(default = "default_poll_secs")]
    #[derivative(Default(value = "default_poll_secs()"))]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    pub poll_secs: u32,

    /// Maximum number of messages to pull in a single request.
    #[serde(default = "default_max_messages")]
    #[derivative(Default(value = "default_max_messages()"))]
    #[configurable(metadata(docs::examples = 10))]
    pub max_messages: u32,

    /// Whether to delete (acknowledge) the message once it is processed.
    ///
    /// It can be useful to set this to `false` for debugging or during the initial setup.
    #[serde(default = "default_true")]
    #[derivative(Default(value = "default_true()"))]
    pub delete_message: bool,

    /// Whether to delete non-retryable messages.
    ///
    /// If a message is rejected by the sink and not retryable, it is acknowledged
    /// to prevent infinite redelivery. Set to `false` to leave failed messages
    /// unacknowledged for manual inspection.
    #[serde(default = "default_true")]
    #[derivative(Default(value = "default_true()"))]
    pub delete_failed_message: bool,

    /// Number of concurrent tasks to create for polling the subscription for messages.
    ///
    /// Defaults to the number of available CPUs on the system.
    #[configurable(metadata(docs::type_unit = "tasks"))]
    #[configurable(metadata(docs::examples = 5))]
    pub client_concurrency: Option<NonZeroUsize>,
}

fn default_pubsub_endpoint() -> String {
    "https://pubsub.googleapis.com".to_string()
}

const fn default_poll_secs() -> u32 {
    5
}

const fn default_max_messages() -> u32 {
    10
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Snafu)]
pub enum ProcessingError {
    #[snafu(display("Failed to pull Pub/Sub messages: {}", source))]
    PullMessages { source: crate::http::HttpError },

    #[snafu(display("Pub/Sub pull returned HTTP {}: {}", status, body))]
    PullMessagesHttp { status: u16, body: String },

    #[snafu(display("Failed to parse Pub/Sub pull response: {}", source))]
    ParsePullResponse { source: serde_json::Error },

    #[snafu(display("Failed to download gs://{}/{}: {}", bucket, object, source))]
    DownloadObject {
        source: crate::http::HttpError,
        bucket: String,
        object: String,
    },

    #[snafu(display(
        "GCS download returned HTTP {} for gs://{}/{}: {}",
        status,
        bucket,
        object,
        body
    ))]
    DownloadObjectHttp {
        status: u16,
        bucket: String,
        object: String,
        body: String,
    },

    #[snafu(display("Failed to read all of gs://{}/{}: {}", bucket, object, source))]
    ReadObject {
        source: Box<dyn FramingError>,
        bucket: String,
        object: String,
    },

    #[snafu(display("Failed to flush all of gs://{}/{}: {}", bucket, object, source))]
    PipelineSend {
        source: SendError,
        bucket: String,
        object: String,
    },

    #[snafu(display("Failed to acknowledge Pub/Sub messages: {}", source))]
    AcknowledgeMessages { source: crate::http::HttpError },

    #[snafu(display("Pub/Sub acknowledge returned HTTP {}: {}", status, body))]
    AcknowledgeMessagesHttp { status: u16, body: String },
}

pub struct State {
    project: String,
    storage_endpoint: String,
    pubsub_endpoint: String,
    subscription: String,

    http_client: HttpClient,
    auth: GcpAuthenticator,

    multiline: Option<line_agg::Config>,
    compression: super::Compression,

    poll_secs: u32,
    max_messages: u32,
    client_concurrency: usize,
    delete_message: bool,
    delete_failed_message: bool,
    decoder: Decoder,
}

pub struct Ingestor {
    state: Arc<State>,
}

impl Ingestor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        project: String,
        storage_endpoint: String,
        http_client: HttpClient,
        auth: GcpAuthenticator,
        config: PubsubConfig,
        compression: super::Compression,
        multiline: Option<line_agg::Config>,
        decoder: Decoder,
    ) -> crate::Result<Ingestor> {
        // Normalize subscription: strip full resource path prefix if provided
        let subscription = config.subscription.rsplit('/').next()
            .unwrap_or(&config.subscription)
            .to_owned();

        let state = Arc::new(State {
            project,
            storage_endpoint,
            pubsub_endpoint: config.endpoint,
            subscription,

            http_client,
            auth,

            compression,
            multiline,

            poll_secs: config.poll_secs,
            max_messages: config.max_messages,
            client_concurrency: config
                .client_concurrency
                .map(|n| n.get())
                .unwrap_or_else(crate::num_threads),
            delete_message: config.delete_message,
            delete_failed_message: config.delete_failed_message,
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
        for _ in 0..self.state.client_concurrency {
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

        for handle in handles.drain(..) {
            if let Err(e) = handle.await
                && e.is_panic()
            {
                panic::resume_unwind(e.into_panic());
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
    log_namespace: LogNamespace,
    bytes_received: Registered<BytesReceived>,
    events_received: Registered<EventsReceived>,
    backoff: ExponentialBackoff,
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
            backoff: ExponentialBackoff::default().max_delay(Duration::from_secs(30)),
        }
    }

    async fn run(mut self) {
        let shutdown = self.shutdown.clone().fuse();
        pin!(shutdown);

        loop {
            select! {
                _ = &mut shutdown => break,
                result = self.run_once() => {
                    let delay = match result {
                        Ok(had_messages) => {
                            self.backoff.reset();
                            if had_messages {
                                None
                            } else {
                                    Some(Duration::from_secs(self.state.poll_secs as u64))
                            }
                        }
                        Err(_) => {
                            Some(self.backoff.next().expect("backoff never ends"))
                        }
                    };
                    if let Some(delay) = delay {
                        select! {
                            _ = &mut shutdown => break,
                            _ = tokio::time::sleep(delay) => {},
                        }
                    }
                },
            }
        }
    }

    /// Returns `Ok(true)` if messages were processed, `Ok(false)` if the poll was empty.
    async fn run_once(&mut self) -> Result<bool, ()> {
        let messages = match self.pull_messages().await {
            Ok(messages) => {
                emit!(GcsPubsubMessageReceiveSucceeded {
                    count: messages.len(),
                });
                messages
            }
            Err(err) => {
                emit!(GcsPubsubMessageReceiveError { error: &err });
                return Err(());
            }
        };

        if messages.is_empty() {
            return Ok(false);
        }

        let mut ack_ids = Vec::new();
        for message in &messages {
            let message_id = message
                .message
                .as_ref()
                .map(|m| m.message_id.as_str())
                .unwrap_or("<unknown>");

            match self.handle_notification(message).await {
                Ok(()) => {
                    emit!(GcsPubsubMessageProcessingSucceeded { message_id });
                    if self.state.delete_message {
                        ack_ids.push(message.ack_id.clone());
                    }
                }
                Err(err) => {
                    emit!(GcsPubsubMessageProcessingError {
                        message_id,
                        error: &err,
                    });
                }
            }
        }

        if !ack_ids.is_empty() {
            match self.acknowledge_messages(&ack_ids).await {
                Ok(()) => {
                    emit!(GcsPubsubMessageAcknowledgeSucceeded {
                        count: ack_ids.len(),
                    });
                }
                Err(err) => {
                    emit!(GcsPubsubMessageAcknowledgeError { error: &err });
                }
            }
        }

        Ok(true)
    }

    async fn handle_notification(
        &mut self,
        received: &PubsubReceivedMessage,
    ) -> Result<(), ProcessingError> {
        let msg = match &received.message {
            Some(msg) => msg,
            None => return Ok(()),
        };

        let attrs = &msg.attributes;
        let bucket_id = attrs.bucket_id.as_deref().unwrap_or("");
        let object_id = attrs.object_id.as_deref().unwrap_or("");
        let event_type = attrs.event_type.as_deref().unwrap_or("");

        if event_type != "OBJECT_FINALIZE" {
            emit!(GcsNotificationInvalidEventIgnored {
                bucket: bucket_id,
                object: object_id,
                event_type,
            });
            return Ok(());
        }

        if bucket_id.is_empty() || object_id.is_empty() {
            warn!(
                message = "GCS notification missing bucketId or objectId attributes.",
                ?attrs,
            );
            return Ok(());
        }

        self.download_and_process_object(bucket_id, object_id)
            .await
    }

    async fn download_and_process_object(
        &mut self,
        bucket: &str,
        object: &str,
    ) -> Result<(), ProcessingError> {
        let download_start = Instant::now();

        let encoded_object =
            percent_encoding::utf8_percent_encode(object, percent_encoding::NON_ALPHANUMERIC)
                .to_string();
        let url = format!(
            "{}/storage/v1/b/{}/o/{}?alt=media",
            self.state.storage_endpoint, bucket, encoded_object
        );

        let mut request = http::Request::get(&url)
            .body(hyper::Body::empty())
            .expect("building GCS download request should not fail");
        self.state.auth.apply(&mut request);

        let response = self
            .state
            .http_client
            .send(request)
            .await
            .map_err(|source| ProcessingError::DownloadObject {
                source,
                bucket: bucket.to_owned(),
                object: object.to_owned(),
            })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body_bytes = body_to_bytes(response.into_body())
                .await
                .unwrap_or_default();
            let body = error_body_string(&body_bytes);
            return Err(ProcessingError::DownloadObjectHttp {
                status,
                bucket: bucket.to_owned(),
                object: object.to_owned(),
                body,
            });
        }

        debug!(
            message = "Downloaded GCS object.",
            bucket = bucket,
            object = object,
        );

        let content_encoding = response
            .headers()
            .get("content-encoding")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());

        let (batch, receiver) = BatchNotifier::maybe_new_with_receiver(self.acknowledgements);

        let object_reader = super::gcs_object_decoder(
            self.state.compression,
            object,
            content_encoding.as_deref(),
            content_type.as_deref(),
            response.into_body(),
        )
        .await;

        let mut read_error = None;
        let bytes_received = self.bytes_received.clone();
        let events_received = self.events_received.clone();
        let lines: Box<dyn Stream<Item = Bytes> + Send + Unpin> = Box::new(
            FramedRead::new(object_reader, self.state.decoder.framer.clone())
                .map(|res| {
                    res.inspect(|bytes| {
                        bytes_received.emit(ByteSize(bytes.len()));
                    })
                    .map_err(|err| {
                        read_error = Some(err);
                    })
                    .ok()
                })
                .take_while(|res| ready(res.is_some()))
                .map(|r| r.expect("validated by take_while")),
        );

        let lines: Box<dyn Stream<Item = Bytes> + Send + Unpin> = match &self.state.multiline {
            Some(config) => Box::new(
                LineAgg::new(
                    lines.map(|line| ((), line, ())),
                    line_agg::Logic::new(config.clone()),
                )
                .map(|(_src, line, _context, _lastline_context)| line),
            ),
            None => lines,
        };

        let bucket_owned = bucket.to_owned();
        let object_owned = object.to_owned();
        let log_namespace = self.log_namespace;
        let mut stream = lines.flat_map(|line| {
            let events = match self.state.decoder.deserializer_parse(line) {
                Ok((events, _events_size)) => events,
                Err(_error) => SmallVec::new(),
            };

            let events = events
                .into_iter()
                .map(|mut event: Event| {
                    event = event.with_batch_notifier_option(&batch);
                    if let Some(log_event) = event.maybe_as_log_mut() {
                        handle_single_log(
                            log_event,
                            log_namespace,
                            &bucket_owned,
                            &object_owned,
                        );
                    }
                    events_received.emit(CountByteSize(1, event.estimated_json_encoded_size_of()));
                    event
                })
                .collect::<Vec<Event>>();
            futures::stream::iter(events)
        });

        let send_error = match self.out.send_event_stream(&mut stream).await {
            Ok(_) => None,
            Err(SendError::Closed) => {
                let (count, _) = stream.size_hint();
                emit!(StreamClosedError { count });
                Some(SendError::Closed)
            }
            Err(SendError::Timeout) => unreachable!("No timeout is configured here"),
        };

        drop(stream);

        let duration = download_start.elapsed();

        if read_error.is_some() {
            emit!(GcsObjectProcessingFailed {
                bucket,
                duration,
            });
        } else {
            emit!(GcsObjectProcessingSucceeded {
                bucket,
                duration,
            });
        }

        drop(batch);

        if let Some(error) = read_error {
            Err(ProcessingError::ReadObject {
                source: error,
                bucket: bucket.to_owned(),
                object: object.to_owned(),
            })
        } else if let Some(error) = send_error {
            Err(ProcessingError::PipelineSend {
                source: error,
                bucket: bucket.to_owned(),
                object: object.to_owned(),
            })
        } else {
            match receiver {
                None => Ok(()),
                Some(receiver) => {
                    let result = receiver.await;
                    match result {
                        BatchStatus::Delivered => {
                            debug!(
                                message = "GCS object delivered.",
                                bucket = bucket,
                                object = object,
                            );
                            Ok(())
                        }
                        BatchStatus::Errored => {
                            warn!(
                                message = "GCS object delivery errored (retryable).",
                                bucket = bucket,
                                object = object,
                            );
                            Err(ProcessingError::PipelineSend {
                                source: SendError::Closed,
                                bucket: bucket.to_owned(),
                                object: object.to_owned(),
                            })
                        }
                        BatchStatus::Rejected => {
                            warn!(
                                message = "GCS object delivery rejected (non-retryable).",
                                bucket = bucket,
                                object = object,
                            );
                            if self.state.delete_failed_message {
                                // Acknowledge to prevent infinite redelivery
                                Ok(())
                            } else {
                                Err(ProcessingError::PipelineSend {
                                    source: SendError::Closed,
                                    bucket: bucket.to_owned(),
                                    object: object.to_owned(),
                                })
                            }
                        }
                    }
                }
            }
        }
    }

    async fn pull_messages(&self) -> Result<Vec<PubsubReceivedMessage>, ProcessingError> {
        let url = format!(
            "{}/v1/projects/{}/subscriptions/{}:pull",
            self.state.pubsub_endpoint, self.state.project, self.state.subscription
        );

        let body = serde_json::json!({
            "maxMessages": self.state.max_messages,
        });

        let mut request = http::Request::post(&url)
            .header("content-type", "application/json")
            .body(hyper::Body::from(serde_json::to_vec(&body).unwrap()))
            .expect("building Pub/Sub pull request should not fail");
        self.state.auth.apply(&mut request);

        let response = self
            .state
            .http_client
            .send(request)
            .await
            .map_err(|source| ProcessingError::PullMessages { source })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body_bytes = body_to_bytes(response.into_body())
                .await
                .unwrap_or_default();
            let body = error_body_string(&body_bytes);
            return Err(ProcessingError::PullMessagesHttp { status, body });
        }

        let body_bytes = body_to_bytes(response.into_body())
            .await
            .map_err(|e| ProcessingError::PullMessagesHttp {
                status: 0,
                body: e.to_string(),
            })?;

        let pull_response: PubsubPullResponse =
            serde_json::from_slice(&body_bytes).map_err(|source| {
                ProcessingError::ParsePullResponse { source }
            })?;

        Ok(pull_response.received_messages.unwrap_or_default())
    }

    async fn acknowledge_messages(&self, ack_ids: &[String]) -> Result<(), ProcessingError> {
        let url = format!(
            "{}/v1/projects/{}/subscriptions/{}:acknowledge",
            self.state.pubsub_endpoint, self.state.project, self.state.subscription
        );

        let body = serde_json::json!({
            "ackIds": ack_ids,
        });

        let mut request = http::Request::post(&url)
            .header("content-type", "application/json")
            .body(hyper::Body::from(serde_json::to_vec(&body).unwrap()))
            .expect("building Pub/Sub acknowledge request should not fail");
        self.state.auth.apply(&mut request);

        let response = self
            .state
            .http_client
            .send(request)
            .await
            .map_err(|source| ProcessingError::AcknowledgeMessages { source })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body_bytes = body_to_bytes(response.into_body())
                .await
                .unwrap_or_default();
            let body = error_body_string(&body_bytes);
            return Err(ProcessingError::AcknowledgeMessagesHttp { status, body });
        }

        Ok(())
    }
}

fn handle_single_log(
    log: &mut LogEvent,
    log_namespace: LogNamespace,
    bucket: &str,
    object: &str,
) {
    log_namespace.insert_source_metadata(
        GcsSourceConfig::NAME,
        log,
        Some(LegacyKey::Overwrite(path!("bucket"))),
        path!("bucket"),
        Bytes::copy_from_slice(bucket.as_bytes()),
    );

    log_namespace.insert_source_metadata(
        GcsSourceConfig::NAME,
        log,
        Some(LegacyKey::Overwrite(path!("object"))),
        path!("object"),
        Bytes::copy_from_slice(object.as_bytes()),
    );

    log_namespace.insert_vector_metadata(
        log,
        log_schema().source_type_key(),
        path!("source_type"),
        Bytes::from_static(GcsSourceConfig::NAME.as_bytes()),
    );

    match log_namespace {
        LogNamespace::Vector => {
            log.insert(metadata_path!("vector", "ingest_timestamp"), Utc::now());
        }
        LogNamespace::Legacy => {
            if let Some(timestamp_key) = log_schema().timestamp_key() {
                log.try_insert((PathPrefix::Event, timestamp_key), Utc::now());
            }
        }
    };
}

fn error_body_string(bytes: &[u8]) -> String {
    const MAX_LEN: usize = 512;
    if bytes.len() > MAX_LEN {
        // Truncate raw bytes before lossy conversion to avoid panic on char boundary.
        format!("{}... (truncated)", String::from_utf8_lossy(&bytes[..MAX_LEN]))
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// hyper Body has ambiguous `.collect()`; call http_body explicitly.
async fn body_to_bytes(body: hyper::Body) -> Result<Bytes, hyper::Error> {
    use http_body::Body as HttpBody;
    HttpBody::collect(body)
        .await
        .map(|collected| collected.to_bytes())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PubsubPullResponse {
    pub received_messages: Option<Vec<PubsubReceivedMessage>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PubsubReceivedMessage {
    pub ack_id: String,
    pub message: Option<PubsubMessage>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PubsubMessage {
    #[serde(default)]
    pub attributes: GcsNotificationAttributes,
    #[serde(default)]
    pub data: Option<String>,
    #[serde(default)]
    pub message_id: String,
    #[serde(default)]
    pub publish_time: Option<String>,
}

/// Attributes set by GCS notifications on Pub/Sub messages.
/// See: https://cloud.google.com/storage/docs/pubsub-notifications#attributes
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GcsNotificationAttributes {
    pub bucket_id: Option<String>,
    pub object_id: Option<String>,
    pub event_type: Option<String>,
    pub object_generation: Option<String>,
    pub payload_format: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pull_response() {
        let json = r#"{
            "receivedMessages": [
                {
                    "ackId": "ack-id-1",
                    "message": {
                        "attributes": {
                            "bucketId": "my-bucket",
                            "objectId": "logs/2024/01/01/file.log.gz",
                            "eventType": "OBJECT_FINALIZE",
                            "objectGeneration": "1234567890"
                        },
                        "data": "eyJ0ZXN0IjogdHJ1ZX0=",
                        "messageId": "msg-123",
                        "publishTime": "2024-01-01T00:00:00Z"
                    }
                }
            ]
        }"#;

        let response: PubsubPullResponse = serde_json::from_str(json).unwrap();
        let messages = response.received_messages.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].ack_id, "ack-id-1");

        let msg = messages[0].message.as_ref().unwrap();
        assert_eq!(msg.attributes.bucket_id.as_deref(), Some("my-bucket"));
        assert_eq!(
            msg.attributes.object_id.as_deref(),
            Some("logs/2024/01/01/file.log.gz")
        );
        assert_eq!(
            msg.attributes.event_type.as_deref(),
            Some("OBJECT_FINALIZE")
        );
    }

    #[test]
    fn test_parse_empty_pull_response() {
        let json = r#"{}"#;
        let response: PubsubPullResponse = serde_json::from_str(json).unwrap();
        assert!(response.received_messages.is_none());
    }

    #[test]
    fn test_parse_pubsub_config() {
        let config: PubsubConfig = toml::from_str(
            r#"
                subscription = "my-subscription"
            "#,
        )
        .unwrap();
        assert_eq!(config.subscription, "my-subscription");
        assert_eq!(config.endpoint, "https://pubsub.googleapis.com");
        assert_eq!(config.poll_secs, 5);
        assert_eq!(config.max_messages, 10);
        assert!(config.delete_message);
    }
}
