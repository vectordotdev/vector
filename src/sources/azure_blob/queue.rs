use std::{
    borrow::Cow,
    collections::HashMap,
    future::ready,
    num::NonZeroUsize,
    panic,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use azure_storage_blob::BlobContainerClient;
use azure_storage_queue::{
    QueueClient,
    models::{QueueClientReceiveMessagesOptions, ReceivedMessage},
};
use base64::prelude::{BASE64_STANDARD, Engine as _};
use bytes::Bytes;
use chrono::{DateTime, TimeZone, Utc};
use futures::{FutureExt, Stream, StreamExt};
use serde::Deserialize;
use smallvec::SmallVec;
use snafu::{ResultExt, Snafu};
use tokio::{pin, select};
use tokio_util::codec::FramedRead;
use vector_lib::{
    codecs::decoding::FramingError,
    config::{LegacyKey, LogNamespace, log_schema},
    configurable::configurable_component,
    event::MaybeAsLogMut,
    internal_event::{
        ByteSize, BytesReceived, CountByteSize, InternalEventHandle as _, Protocol, Registered,
        error_type,
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
    internal_events::{
        AzureBlobEventIgnored, AzureBlobProcessingFailed, AzureBlobProcessingSucceeded,
        AzureQueueMessageDeleteError, AzureQueueMessageDeleteSucceeded,
        AzureQueueMessageProcessingError, AzureQueueMessageProcessingSucceeded,
        AzureQueueMessageReceiveError, AzureQueueMessageReceiveSucceeded, EventsReceived,
        StreamClosedError,
    },
    line_agg::{self, LineAgg},
    shutdown::ShutdownSignal,
    sources::azure_blob::{AzureBlobConfig, AzureStorageClientSource},
};

/// The Event Grid event type that triggers ingestion.
const BLOB_CREATED_EVENT_TYPE: &str = "Microsoft.Storage.BlobCreated";

/// The prefix of the Event Grid `subject` field for blob events.
const SUBJECT_CONTAINER_PREFIX: &str = "/blobServices/default/containers/";

/// The separator between container and blob path in the Event Grid `subject` field.
const SUBJECT_BLOB_SEPARATOR: &str = "/blobs/";

/// Azure Storage Queue configuration options.
#[configurable_component]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub(super) struct Config {
    /// The name of the Storage Queue that receives the `Microsoft.Storage.BlobCreated`
    /// notifications from the Event Grid subscription.
    ///
    /// This is a queue name, not a URL; the full URL is derived from the queue service endpoint.
    #[configurable(metadata(docs::examples = "vector-blob-events"))]
    pub(super) queue_name: String,

    /// Maximum time to wait between polls of the queue when it is empty, in seconds.
    ///
    /// Azure Storage Queues have no server-side long polling, so an exponential client-side
    /// backoff (starting at one second) is applied between empty polls, capped at this value.
    /// Polling resumes immediately whenever a poll returns at least one message.
    ///
    /// Must be at least `1`.
    #[serde(default = "default_poll_secs")]
    #[derivative(Default(value = "default_poll_secs()"))]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    pub(super) poll_secs: u32,

    /// The visibility timeout to use for messages, in seconds.
    ///
    /// This controls how long a message is left unavailable after it is received. If a message
    /// is received, and takes longer than `visibility_timeout_secs` to process and delete the
    /// message from the queue, it is made available again for another consumer.
    ///
    /// This can happen if there is an issue between consuming a message and deleting it.
    // NOTE: We restrict this to u32 for safe conversion to i32 later.
    #[serde(default = "default_visibility_timeout_secs")]
    #[derivative(Default(value = "default_visibility_timeout_secs()"))]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::human_name = "Visibility Timeout"))]
    pub(super) visibility_timeout_secs: u32,

    /// Maximum number of messages to poll from the queue in a batch.
    ///
    /// Should be set to a smaller value when the blobs are large to help prevent the ingestion
    /// of one blob from causing the others to exceed the `visibility_timeout_secs`. Valid
    /// values are 1 - 32.
    // NOTE: We restrict this to u32 for safe conversion to i32 later.
    #[serde(default = "default_max_number_of_messages")]
    #[derivative(Default(value = "default_max_number_of_messages()"))]
    #[configurable(metadata(docs::human_name = "Max Messages"))]
    #[configurable(metadata(docs::examples = 1))]
    pub(super) max_number_of_messages: u32,

    /// Number of concurrent tasks to create for polling the queue for messages.
    ///
    /// Defaults to the number of available CPUs on the system.
    ///
    /// Should not typically need to be changed, but it can sometimes be beneficial to raise this
    /// value when there is a high rate of messages being pushed into the queue and the blobs
    /// being fetched are small. In these cases, system resources may not be fully utilized
    /// without fetching more messages per second, as the queue message consumption rate affects
    /// the blob retrieval rate.
    #[configurable(metadata(docs::type_unit = "tasks"))]
    #[configurable(metadata(docs::examples = 5))]
    pub(super) client_concurrency: Option<NonZeroUsize>,

    /// Whether to delete the message once it is processed.
    ///
    /// It can be useful to set this to `false` for debugging or during the initial setup.
    #[serde(default = "default_true")]
    #[derivative(Default(value = "default_true()"))]
    pub(super) delete_message: bool,

    /// Whether to delete non-retryable messages.
    ///
    /// If a message is rejected by the sink and not retryable, it is deleted from the queue.
    /// With no dead-letter queue support, setting this to `false` means rejected messages are
    /// redelivered indefinitely.
    #[serde(default = "default_true")]
    #[derivative(Default(value = "default_true()"))]
    pub(super) delete_failed_message: bool,
}

const fn default_poll_secs() -> u32 {
    15
}

const fn default_visibility_timeout_secs() -> u32 {
    300
}

const fn default_max_number_of_messages() -> u32 {
    10
}

const fn default_true() -> bool {
    true
}

/// The visibility timeout range permitted by the Queue Storage service: 1 second to 7 days.
const VISIBILITY_TIMEOUT_SECS_RANGE: std::ops::RangeInclusive<u32> = 1..=(7 * 24 * 60 * 60);

#[derive(Debug, Snafu)]
pub(super) enum IngestorNewError {
    #[snafu(display(
        "Invalid value for max_number_of_messages {}, valid values are 1 - 32",
        messages
    ))]
    InvalidNumberOfMessages { messages: u32 },
    #[snafu(display(
        "Invalid value for visibility_timeout_secs {}, valid values are 1 second - 7 days",
        seconds
    ))]
    InvalidVisibilityTimeout { seconds: u32 },
    #[snafu(display("Invalid value for poll_secs 0, must be at least 1 second"))]
    ZeroPollSecs,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Snafu)]
pub enum ProcessingError {
    #[snafu(display(
        "Could not parse queue message with id {} as a blob notification: {}",
        message_id,
        source
    ))]
    InvalidQueueMessage {
        source: serde_json::Error,
        message_id: String,
    },
    #[snafu(display(
        "Could not resolve container and blob from notification subject {:?} and url {:?}",
        subject,
        url
    ))]
    InvalidBlobPath {
        subject: Option<String>,
        url: Option<String>,
    },
    #[snafu(display(
        "Received notification for storage account '{received}', but source is configured for '{configured}'"
    ))]
    ForeignStorageAccount {
        configured: String,
        received: String,
    },
    #[snafu(display("Failed to build client for container {}: {}", container, message))]
    ContainerClient { message: String, container: String },
    #[snafu(display("Failed to fetch blob {}/{}: {}", container, blob, source))]
    GetBlob {
        source: azure_core::Error,
        container: String,
        blob: String,
    },
    #[snafu(display("Failed to read all of blob {}/{}: {}", container, blob, source))]
    ReadBlob {
        source: Box<dyn FramingError>,
        container: String,
        blob: String,
    },
    #[snafu(display("Failed to flush all of blob {}/{}: {}", container, blob, source))]
    PipelineSend {
        source: vector_lib::source_sender::SendError,
        container: String,
        blob: String,
    },
    #[snafu(display(
        "Sink reported an error sending events for blob {}/{}",
        container,
        blob
    ))]
    ErrorAcknowledgement { container: String, blob: String },
}

impl ProcessingError {
    pub const fn error_type(&self) -> &'static str {
        match self {
            Self::InvalidQueueMessage { .. } | Self::InvalidBlobPath { .. } => {
                error_type::PARSER_FAILED
            }
            Self::ForeignStorageAccount { .. } | Self::ContainerClient { .. } => {
                error_type::CONFIGURATION_FAILED
            }
            Self::GetBlob { .. } => error_type::REQUEST_FAILED,
            Self::ReadBlob { .. } => error_type::READER_FAILED,
            Self::PipelineSend { .. } => error_type::WRITER_FAILED,
            Self::ErrorAcknowledgement { .. } => error_type::ACKNOWLEDGMENT_FAILED,
        }
    }

    pub const fn is_non_retryable(&self) -> bool {
        matches!(
            self,
            Self::InvalidQueueMessage { .. }
                | Self::InvalidBlobPath { .. }
                | Self::ForeignStorageAccount { .. }
        )
    }
}

pub struct State {
    account_name: Option<String>,
    clients: AzureStorageClientSource,
    queue_client: QueueClient,
    container_clients: RwLock<HashMap<String, Arc<BlobContainerClient>>>,

    multiline: Option<line_agg::Config>,
    compression: super::Compression,

    poll_secs: u32,
    visibility_timeout_secs: i32,
    max_number_of_messages: i32,
    client_concurrency: usize,
    delete_message: bool,
    delete_failed_message: bool,
    decoder: Decoder,
}

impl State {
    /// Event Grid subscriptions are account-scoped, so a single queue can carry notifications
    /// for blobs in any container of the storage account.
    fn container_client(
        &self,
        container: &str,
    ) -> Result<Arc<BlobContainerClient>, ProcessingError> {
        if let Some(client) = self
            .container_clients
            .read()
            .expect("lock poisoned")
            .get(container)
        {
            return Ok(Arc::clone(client));
        }

        let client = self.clients.container_client(container).map_err(|error| {
            ProcessingError::ContainerClient {
                message: error.to_string(),
                container: container.to_owned(),
            }
        })?;

        let mut clients = self.container_clients.write().expect("lock poisoned");
        let entry = clients
            .entry(container.to_owned())
            .or_insert_with(|| Arc::new(client));
        Ok(Arc::clone(entry))
    }
}

pub(super) struct Ingestor {
    state: Arc<State>,
}

impl Ingestor {
    pub(super) fn new(
        clients: AzureStorageClientSource,
        config: Config,
        compression: super::Compression,
        multiline: Option<line_agg::Config>,
        decoder: Decoder,
    ) -> crate::Result<Ingestor> {
        if config.max_number_of_messages < 1 || config.max_number_of_messages > 32 {
            return Err(IngestorNewError::InvalidNumberOfMessages {
                messages: config.max_number_of_messages,
            }
            .into());
        }
        if !VISIBILITY_TIMEOUT_SECS_RANGE.contains(&config.visibility_timeout_secs) {
            return Err(IngestorNewError::InvalidVisibilityTimeout {
                seconds: config.visibility_timeout_secs,
            }
            .into());
        }
        if config.poll_secs == 0 {
            return Err(IngestorNewError::ZeroPollSecs.into());
        }

        let queue_client = clients.queue_client(&config.queue_name)?;
        let account_name = clients.account_name();

        let state = Arc::new(State {
            account_name,
            clients,
            queue_client,
            container_clients: RwLock::new(HashMap::new()),

            compression,
            multiline,

            poll_secs: config.poll_secs,
            visibility_timeout_secs: config.visibility_timeout_secs as i32,
            max_number_of_messages: config.max_number_of_messages as i32,
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

    pub(super) async fn run(
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
            let handle = crate::spawn_in_current_span(fut);
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
    error_backoff: ExponentialBackoff,
    empty_backoff: ExponentialBackoff,
}

impl IngestorProcess {
    pub fn new(
        state: Arc<State>,
        out: SourceSender,
        shutdown: ShutdownSignal,
        log_namespace: LogNamespace,
        acknowledgements: bool,
    ) -> Self {
        // `GetMessages` has no server-side long poll, so empty polls back off client-side.
        let empty_backoff = ExponentialBackoff::from_millis(2)
            .factor(500)
            .max_delay(Duration::from_secs(state.poll_secs.into()));

        Self {
            state,
            out,
            shutdown,
            acknowledgements,
            log_namespace,
            bytes_received: register!(BytesReceived::from(Protocol::HTTPS)),
            events_received: register!(EventsReceived),
            error_backoff: ExponentialBackoff::default().max_delay(Duration::from_secs(30)),
            empty_backoff,
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
                        Ok(received) => {
                            self.error_backoff.reset();
                            if received > 0 {
                                self.empty_backoff.reset();
                                None
                            } else {
                                Some(self.empty_backoff.next().expect("backoff never ends"))
                            }
                        }
                        Err(()) => Some(self.error_backoff.next().expect("backoff never ends")),
                    };
                    if let Some(delay) = delay {
                        trace!(
                            delay_ms = delay.as_millis(),
                            "Waiting before polling the queue again.",
                        );
                        select! {
                            _ = &mut shutdown => break,
                            _ = tokio::time::sleep(delay) => {},
                        }
                    }
                },
            }
        }
    }

    async fn run_once(&mut self) -> Result<usize, ()> {
        let messages = match self.receive_messages().await {
            Ok(messages) => {
                emit!(AzureQueueMessageReceiveSucceeded {
                    count: messages.len(),
                });
                messages
            }
            Err(err) => {
                emit!(AzureQueueMessageReceiveError { error: &err });
                return Err(());
            }
        };

        let count = messages.len();
        for message in messages {
            self.handle_message(message).await;
        }

        Ok(count)
    }

    async fn handle_message(&mut self, message: ReceivedMessage) {
        let message_id = message
            .message_id
            .clone()
            .unwrap_or_else(|| "<unknown>".to_owned());
        let Some(pop_receipt) = message.pop_receipt.clone() else {
            warn!(
                message = "Refusing to process message with no pop_receipt.",
                message_id = %message_id,
            );
            return;
        };
        let dequeue_count = message.dequeue_count;

        match self.handle_queue_message(message).await {
            Ok(()) => {
                emit!(AzureQueueMessageProcessingSucceeded {
                    message_id: &message_id,
                });
                if self.state.delete_message {
                    self.delete_message(&message_id, &pop_receipt).await;
                }
            }
            Err(err) => {
                emit!(AzureQueueMessageProcessingError {
                    message_id: &message_id,
                    error: &err,
                    dequeue_count,
                });
                if self.state.delete_failed_message && err.is_non_retryable() {
                    warn!(
                        message = "Deleting non-retryable failed queue message.",
                        message_id = %message_id,
                        error = %err,
                    );
                    self.delete_message(&message_id, &pop_receipt).await;
                }
            }
        }
    }

    async fn handle_queue_message(
        &mut self,
        message: ReceivedMessage,
    ) -> Result<(), ProcessingError> {
        let body = message.message_text.unwrap_or_default();
        let body = decode_message_text(&body);

        let event: QueueEvent =
            serde_json::from_str(body.as_ref()).context(InvalidQueueMessageSnafu {
                message_id: message
                    .message_id
                    .clone()
                    .unwrap_or_else(|| "<empty>".to_owned()),
            })?;

        for notification in event.into_notifications() {
            self.handle_blob_notification(notification).await?;
        }
        Ok(())
    }

    async fn handle_blob_notification(
        &mut self,
        notification: BlobNotification,
    ) -> Result<(), ProcessingError> {
        if notification.event_type != BLOB_CREATED_EVENT_TYPE {
            emit!(AzureBlobEventIgnored {
                event_type: &notification.event_type,
            });
            return Ok(());
        }

        let blob_ref =
            resolve_blob_ref(&notification).ok_or_else(|| ProcessingError::InvalidBlobPath {
                subject: notification.subject.clone(),
                url: notification.url.clone(),
            })?;

        if let (Some(configured), Some(received)) = (
            self.state.account_name.as_deref(),
            blob_ref.storage_account.as_deref(),
        ) && !configured.eq_ignore_ascii_case(received)
        {
            return Err(ProcessingError::ForeignStorageAccount {
                configured: configured.to_owned(),
                received: received.to_owned(),
            });
        }

        let container_client = self.state.container_client(&blob_ref.container)?;

        let download_start = Instant::now();

        let object = container_client
            .blob_client(&blob_ref.blob)
            .download(None)
            .await
            .context(GetBlobSnafu {
                container: blob_ref.container.clone(),
                blob: blob_ref.blob.clone(),
            })?;

        debug!(
            message = "Got blob from queue notification.",
            container = blob_ref.container,
            blob = blob_ref.blob,
        );

        let metadata = object.properties.metadata;

        let timestamp = object
            .properties
            .last_modified
            .and_then(to_chrono_timestamp)
            .or(notification.event_time);

        let (batch, receiver) = BatchNotifier::maybe_new_with_receiver(self.acknowledgements);
        let object_reader = super::blob_decoder(
            self.state.compression,
            &blob_ref.blob,
            object.properties.content_encoding.as_deref(),
            object.properties.content_type.as_deref(),
            object.body,
        )
        .await;

        // Record the read error seen to propagate up later so we avoid ack'ing the queue
        // message
        //
        // String is used as we cannot clone std::io::Error to take ownership in closure
        //
        // FramedRead likely stops when it gets an i/o error but I found it more clear to
        // show that we `take_while` there hasn't been an error
        //
        // This can result in blobs being partially processed before an error, but we
        // prefer duplicate lines over message loss. Future work could include recording
        // the offset of the blob that has been read, but this would only be relevant in
        // the case that the same vector instance processes the same message.
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

        let log_namespace = self.log_namespace;
        let mut stream = lines.flat_map(|line| {
            let events = match self.state.decoder.deserializer_parse(line) {
                Ok((events, _events_size)) => events,
                Err(_error) => {
                    // Error is handled by `codecs::Decoder`, no further handling
                    // is needed here.
                    SmallVec::new()
                }
            };

            let events = events
                .into_iter()
                .map(|mut event: Event| {
                    event = event.with_batch_notifier_option(&batch);
                    if let Some(log_event) = event.maybe_as_log_mut() {
                        handle_single_log(
                            log_event,
                            log_namespace,
                            &blob_ref,
                            &metadata,
                            timestamp,
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
        drop(batch);

        // Deliberately not the same as `result.is_ok()`: a rejected batch is removed from the
        // queue when `delete_failed_message` is set, but nothing was delivered.
        let mut delivered = false;
        let container = blob_ref.container.clone();

        let result = if let Some(error) = read_error {
            Err(ProcessingError::ReadBlob {
                source: error,
                container: blob_ref.container.clone(),
                blob: blob_ref.blob.clone(),
            })
        } else if let Some(error) = send_error {
            Err(ProcessingError::PipelineSend {
                source: error,
                container: blob_ref.container.clone(),
                blob: blob_ref.blob.clone(),
            })
        } else {
            match receiver {
                None => {
                    delivered = true;
                    Ok(())
                }
                Some(receiver) => match receiver.await {
                    BatchStatus::Delivered => {
                        delivered = true;
                        debug!(
                            message = "Blob from queue notification delivered.",
                            container = blob_ref.container,
                            blob = blob_ref.blob,
                        );
                        Ok(())
                    }
                    BatchStatus::Errored => Err(ProcessingError::ErrorAcknowledgement {
                        container: blob_ref.container,
                        blob: blob_ref.blob,
                    }),
                    BatchStatus::Rejected => {
                        if self.state.delete_failed_message {
                            warn!(
                                message = "Blob from queue notification was rejected. Deleting failed message.",
                                container = blob_ref.container,
                                blob = blob_ref.blob,
                            );
                            Ok(())
                        } else {
                            Err(ProcessingError::ErrorAcknowledgement {
                                container: blob_ref.container,
                                blob: blob_ref.blob,
                            })
                        }
                    }
                },
            }
        };

        let duration = download_start.elapsed();
        if delivered {
            emit!(AzureBlobProcessingSucceeded {
                container: &container,
                duration
            });
        } else {
            emit!(AzureBlobProcessingFailed {
                container: &container,
                duration
            });
        }

        result
    }

    async fn receive_messages(&mut self) -> azure_core::Result<Vec<ReceivedMessage>> {
        let options = QueueClientReceiveMessagesOptions {
            number_of_messages: Some(self.state.max_number_of_messages),
            visibility_timeout: Some(self.state.visibility_timeout_secs),
            ..Default::default()
        };
        let response = self
            .state
            .queue_client
            .receive_messages(Some(options))
            .await?;
        Ok(response.into_model()?.items.unwrap_or_default())
    }

    /// Delete a single message. There is no batch-delete API in Queue Storage.
    ///
    /// A failure from a stale pop receipt is benign: the message redelivers and the blob is
    /// ingested again, consistent with the source's at-least-once semantics.
    async fn delete_message(&mut self, message_id: &str, pop_receipt: &str) {
        match self
            .state
            .queue_client
            .delete_message(message_id, pop_receipt, None)
            .await
        {
            Ok(_) => {
                emit!(AzureQueueMessageDeleteSucceeded { message_id });
            }
            Err(err) => {
                emit!(AzureQueueMessageDeleteError {
                    message_id,
                    error: &err,
                });
            }
        }
    }
}

fn handle_single_log(
    log: &mut LogEvent,
    log_namespace: LogNamespace,
    blob_ref: &BlobRef,
    metadata: &HashMap<String, String>,
    timestamp: Option<DateTime<Utc>>,
) {
    log_namespace.insert_source_metadata(
        AzureBlobConfig::NAME,
        log,
        Some(LegacyKey::Overwrite(path!("container"))),
        path!("container"),
        Bytes::from(blob_ref.container.as_bytes().to_vec()),
    );

    log_namespace.insert_source_metadata(
        AzureBlobConfig::NAME,
        log,
        Some(LegacyKey::Overwrite(path!("blob"))),
        path!("blob"),
        Bytes::from(blob_ref.blob.as_bytes().to_vec()),
    );

    if let Some(storage_account) = &blob_ref.storage_account {
        log_namespace.insert_source_metadata(
            AzureBlobConfig::NAME,
            log,
            Some(LegacyKey::Overwrite(path!("storage_account"))),
            path!("storage_account"),
            Bytes::from(storage_account.as_bytes().to_vec()),
        );
    }

    for (key, value) in metadata {
        log_namespace.insert_source_metadata(
            AzureBlobConfig::NAME,
            log,
            Some(LegacyKey::Overwrite(path!(key))),
            path!("metadata", key.as_str()),
            value.clone(),
        );
    }

    log_namespace.insert_vector_metadata(
        log,
        log_schema().source_type_key(),
        path!("source_type"),
        Bytes::from_static(AzureBlobConfig::NAME.as_bytes()),
    );

    // The blob's `Last-Modified` time, falling back to the notification's event time, and
    // finally (for the Legacy namespace) to `now()`.
    match log_namespace {
        LogNamespace::Vector => {
            if let Some(timestamp) = timestamp {
                log.insert(
                    metadata_path!(AzureBlobConfig::NAME, "timestamp"),
                    timestamp,
                );
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
}

/// Event Grid base64-encodes the JSON event when delivering to a Storage Queue, but manual or
/// test messages may be raw JSON. Trying base64 first is unambiguous because `{`, the first
/// character of any JSON object, is not in the base64 alphabet.
fn decode_message_text(raw: &str) -> Cow<'_, str> {
    match BASE64_STANDARD.decode(raw.trim()) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(s) => Cow::Owned(s),
            Err(_) => Cow::Borrowed(raw),
        },
        Err(_) => Cow::Borrowed(raw),
    }
}

/// A blob notification in either of the two subscription schemas, auto-detected.
///
/// The two object variants are mutually exclusive on required fields, so their relative order
/// does not matter. `EventGridBatch` must stay last: it is the only sequence variant, and
/// matching it against an object is what an untagged enum falls through to.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum QueueEvent {
    CloudEvent(CloudEventEnvelope),
    EventGrid(EventGridEnvelope),
    EventGridBatch(Vec<EventGridEnvelope>),
}

impl QueueEvent {
    fn into_notifications(self) -> Vec<BlobNotification> {
        match self {
            QueueEvent::CloudEvent(event) => {
                if !event.specversion.starts_with("1.") {
                    warn!(
                        message = "Unexpected CloudEvents specversion, processing anyway.",
                        specversion = %event.specversion,
                    );
                }
                vec![event.into()]
            }
            QueueEvent::EventGrid(event) => vec![event.into()],
            QueueEvent::EventGridBatch(events) => events.into_iter().map(Into::into).collect(),
        }
    }
}

// https://learn.microsoft.com/azure/event-grid/event-schema-blob-storage
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventGridEnvelope {
    topic: Option<String>,
    event_type: String,
    subject: String,
    event_time: Option<DateTime<Utc>>,
    data: Option<BlobEventData>,
}

impl From<EventGridEnvelope> for BlobNotification {
    fn from(event: EventGridEnvelope) -> Self {
        let storage_account = event
            .data
            .as_ref()
            .and_then(|data| data.url.as_deref().and_then(account_from_url))
            .or_else(|| event.topic.as_deref().and_then(account_from_resource_id));

        BlobNotification {
            event_type: event.event_type,
            subject: Some(event.subject),
            event_time: event.event_time,
            url: event.data.and_then(|data| data.url),
            storage_account,
        }
    }
}

// https://learn.microsoft.com/azure/event-grid/cloud-event-schema
#[derive(Clone, Debug, Deserialize)]
struct CloudEventEnvelope {
    specversion: String,
    #[serde(rename = "type")]
    event_type: String,
    source: Option<String>,
    subject: Option<String>,
    time: Option<DateTime<Utc>>,
    data: Option<BlobEventData>,
}

impl From<CloudEventEnvelope> for BlobNotification {
    fn from(event: CloudEventEnvelope) -> Self {
        let storage_account = event
            .data
            .as_ref()
            .and_then(|data| data.url.as_deref().and_then(account_from_url))
            .or_else(|| event.source.as_deref().and_then(account_from_resource_id));

        BlobNotification {
            event_type: event.event_type,
            subject: event.subject,
            event_time: event.time,
            url: event.data.and_then(|data| data.url),
            storage_account,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlobEventData {
    url: Option<String>,
}

#[derive(Clone, Debug)]
struct BlobNotification {
    event_type: String,
    subject: Option<String>,
    event_time: Option<DateTime<Utc>>,
    url: Option<String>,
    storage_account: Option<String>,
}

/// The identity of a blob resolved from a notification.
#[derive(Clone, Debug, PartialEq, Eq)]
struct BlobRef {
    storage_account: Option<String>,
    container: String,
    blob: String,
}

fn resolve_blob_ref(notification: &BlobNotification) -> Option<BlobRef> {
    if let Some(subject) = notification.subject.as_deref()
        && let Some((container, blob)) = parse_subject(subject)
    {
        return Some(BlobRef {
            storage_account: notification
                .storage_account
                .clone()
                .or_else(|| notification.url.as_deref().and_then(account_from_url)),
            container,
            blob,
        });
    }

    notification.url.as_deref().and_then(parse_blob_url)
}

fn parse_subject(subject: &str) -> Option<(String, String)> {
    let rest = subject.strip_prefix(SUBJECT_CONTAINER_PREFIX)?;
    let (container, blob) = rest.split_once(SUBJECT_BLOB_SEPARATOR)?;
    if container.is_empty() || blob.is_empty() {
        return None;
    }
    Some((container.to_owned(), blob.to_owned()))
}

fn account_from_resource_id(resource_id: &str) -> Option<String> {
    let mut segments = resource_id.split('/');
    while let Some(seg) = segments.next() {
        if seg.eq_ignore_ascii_case("storageAccounts") {
            return segments
                .next()
                .filter(|account| !account.is_empty())
                .map(ToOwned::to_owned);
        }
    }
    None
}

fn is_cloud_storage_host(host: &str) -> bool {
    host.contains(".blob.") || host.contains(".dfs.")
}

pub(super) fn account_from_url(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    if is_cloud_storage_host(host) {
        return host.split('.').next().map(ToOwned::to_owned);
    }
    parsed
        .path_segments()?
        .next()
        .filter(|segment| !segment.is_empty())
        .map(percent_decode)
}

fn parse_blob_url(url: &str) -> Option<BlobRef> {
    let parsed = url::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    let segments: Vec<&str> = parsed.path_segments()?.collect();

    let (account, container, blob_segments) = if is_cloud_storage_host(host) {
        let account = host.split('.').next()?;
        let (container, blob_segments) = segments.split_first()?;
        (account.to_owned(), *container, blob_segments)
    } else {
        let (account, rest) = segments.split_first()?;
        let (container, blob_segments) = rest.split_first()?;
        ((*account).to_owned(), *container, blob_segments)
    };

    if container.is_empty() || blob_segments.is_empty() {
        return None;
    }

    let blob = blob_segments
        .iter()
        .map(|segment| percent_decode(segment))
        .collect::<Vec<_>>()
        .join("/");

    Some(BlobRef {
        storage_account: Some(percent_decode(&account)),
        container: percent_decode(container),
        blob,
    })
}

fn percent_decode(s: &str) -> String {
    percent_encoding::percent_decode_str(s)
        .decode_utf8_lossy()
        .into_owned()
}

fn to_chrono_timestamp(ts: azure_core::time::OffsetDateTime) -> Option<DateTime<Utc>> {
    Utc.timestamp_opt(ts.unix_timestamp(), ts.nanosecond())
        .single()
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVENT_GRID_BODY: &str = r#"{
        "topic": "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Storage/storageAccounts/myacct",
        "subject": "/blobServices/default/containers/logs/blobs/app/out.log",
        "eventType": "Microsoft.Storage.BlobCreated",
        "eventTime": "2026-06-01T12:00:00.000Z",
        "id": "00000000-0000-0000-0000-000000000000",
        "data": {
            "api": "PutBlob",
            "contentType": "text/plain",
            "contentLength": 42,
            "blobType": "BlockBlob",
            "url": "https://myacct.blob.core.windows.net/logs/app/out.log",
            "eTag": "0x8DC0000000000000"
        },
        "dataVersion": "",
        "metadataVersion": "1"
    }"#;

    const CLOUD_EVENT_BODY: &str = r#"{
        "specversion": "1.0",
        "type": "Microsoft.Storage.BlobCreated",
        "source": "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Storage/storageAccounts/myacct",
        "subject": "/blobServices/default/containers/logs/blobs/app/out.log",
        "time": "2026-06-01T12:00:00.000Z",
        "id": "00000000-0000-0000-0000-000000000000",
        "data": {
            "api": "PutBlob",
            "contentType": "text/plain",
            "contentLength": 42,
            "blobType": "BlockBlob",
            "url": "https://myacct.blob.core.windows.net/logs/app/out.log",
            "eTag": "0x8DC0000000000000"
        }
    }"#;

    fn parse(body: &str) -> Vec<BlobNotification> {
        serde_json::from_str::<QueueEvent>(body)
            .unwrap()
            .into_notifications()
    }

    #[test]
    fn parses_event_grid_schema() {
        let notifications = parse(EVENT_GRID_BODY);
        assert_eq!(notifications.len(), 1);
        let notification = &notifications[0];
        assert_eq!(notification.event_type, BLOB_CREATED_EVENT_TYPE);
        assert!(matches!(
            serde_json::from_str::<QueueEvent>(EVENT_GRID_BODY).unwrap(),
            QueueEvent::EventGrid(_)
        ));
        assert_eq!(
            resolve_blob_ref(notification),
            Some(BlobRef {
                storage_account: Some("myacct".to_owned()),
                container: "logs".to_owned(),
                blob: "app/out.log".to_owned(),
            })
        );
    }

    #[test]
    fn parses_cloud_events_schema() {
        let notifications = parse(CLOUD_EVENT_BODY);
        assert_eq!(notifications.len(), 1);
        assert!(matches!(
            serde_json::from_str::<QueueEvent>(CLOUD_EVENT_BODY).unwrap(),
            QueueEvent::CloudEvent(_)
        ));
        assert_eq!(notifications[0].event_type, BLOB_CREATED_EVENT_TYPE);
    }

    #[test]
    fn parses_event_grid_array() {
        let body = format!("[{EVENT_GRID_BODY}]");
        let notifications = parse(&body);
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].event_type, BLOB_CREATED_EVENT_TYPE);
    }

    #[test]
    fn parses_base64_encoded_body() {
        let encoded = BASE64_STANDARD.encode(EVENT_GRID_BODY);
        let decoded = decode_message_text(&encoded);
        let notifications = parse(&decoded);
        assert_eq!(notifications.len(), 1);
    }

    #[test]
    fn passes_through_raw_body() {
        let decoded = decode_message_text(EVENT_GRID_BODY);
        assert_eq!(decoded.as_ref(), EVENT_GRID_BODY);
    }

    #[test]
    fn rejects_garbage_body() {
        assert!(serde_json::from_str::<QueueEvent>("not json").is_err());
    }

    #[test]
    fn ignores_other_event_types() {
        let body = EVENT_GRID_BODY.replace(
            "Microsoft.Storage.BlobCreated",
            "Microsoft.Storage.BlobDeleted",
        );
        let notifications = parse(&body);
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].event_type, "Microsoft.Storage.BlobDeleted");
    }

    #[test]
    fn subject_extraction() {
        assert_eq!(
            parse_subject("/blobServices/default/containers/logs/blobs/app/out.log"),
            Some(("logs".to_owned(), "app/out.log".to_owned()))
        );
        assert_eq!(
            parse_subject("/blobServices/default/containers/logs/blobs/file name.log"),
            Some(("logs".to_owned(), "file name.log".to_owned()))
        );
        assert_eq!(
            parse_subject("/blobServices/default/containers/logs/blobs/2026%2Fjan.log"),
            Some(("logs".to_owned(), "2026%2Fjan.log".to_owned()))
        );
        assert_eq!(parse_subject("/blobServices/default/containers/logs"), None);
        assert_eq!(parse_subject("unrelated"), None);
    }

    #[test]
    fn url_extraction_cloud_style() {
        assert_eq!(
            parse_blob_url("https://myacct.blob.core.windows.net/logs/app/out.log"),
            Some(BlobRef {
                storage_account: Some("myacct".to_owned()),
                container: "logs".to_owned(),
                blob: "app/out.log".to_owned(),
            })
        );
    }

    #[test]
    fn url_extraction_path_style() {
        assert_eq!(
            parse_blob_url("http://127.0.0.1:10000/devstoreaccount1/logs/app/out.log"),
            Some(BlobRef {
                storage_account: Some("devstoreaccount1".to_owned()),
                container: "logs".to_owned(),
                blob: "app/out.log".to_owned(),
            })
        );
    }

    #[test]
    fn url_extraction_percent_encoded() {
        assert_eq!(
            parse_blob_url("https://myacct.blob.core.windows.net/logs/file%20name.log"),
            Some(BlobRef {
                storage_account: Some("myacct".to_owned()),
                container: "logs".to_owned(),
                blob: "file name.log".to_owned(),
            })
        );
    }

    #[test]
    fn subject_takes_precedence_over_url() {
        let notification = BlobNotification {
            event_type: BLOB_CREATED_EVENT_TYPE.to_owned(),
            subject: Some("/blobServices/default/containers/from-subject/blobs/a.log".to_owned()),
            event_time: None,
            url: Some("https://myacct.blob.core.windows.net/from-url/b.log".to_owned()),
            storage_account: None,
        };
        let blob_ref = resolve_blob_ref(&notification).unwrap();
        assert_eq!(blob_ref.container, "from-subject");
        assert_eq!(blob_ref.blob, "a.log");
        assert_eq!(blob_ref.storage_account.as_deref(), Some("myacct"));
    }

    #[test]
    fn parse_queue_config() {
        let config: Config = serde_yaml::from_str(
            r#"queue_name: "my-queue"
"#,
        )
        .unwrap();
        assert_eq!(config.queue_name, "my-queue");
        assert_eq!(config.poll_secs, 15);
        assert_eq!(config.visibility_timeout_secs, 300);
        assert_eq!(config.max_number_of_messages, 10);
        assert!(config.delete_message);
        assert!(config.delete_failed_message);
    }

    #[test]
    fn url_extraction_dfs_style() {
        assert_eq!(
            account_from_url("https://myacct.dfs.core.windows.net/filesystem/app/out.log"),
            Some("myacct".to_owned())
        );
        assert_eq!(
            parse_blob_url("https://myacct.dfs.core.windows.net/filesystem/app/out.log"),
            Some(BlobRef {
                storage_account: Some("myacct".to_owned()),
                container: "filesystem".to_owned(),
                blob: "app/out.log".to_owned(),
            })
        );
    }

    #[test]
    fn arm_resource_id_extraction() {
        assert_eq!(
            account_from_resource_id(
                "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Storage/storageAccounts/myacct"
            ),
            Some("myacct".to_owned())
        );
        assert_eq!(
            account_from_resource_id(
                "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Storage/storageAccounts/myacct/blobServices/default"
            ),
            Some("myacct".to_owned())
        );
        assert_eq!(
            account_from_resource_id("/subscriptions/00000000-0000-0000-0000-000000000000"),
            None
        );
    }

    #[test]
    fn extracts_account_from_topic_or_source_fallback() {
        let event_grid_json = r#"{
            "topic": "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Storage/storageAccounts/topicacct",
            "subject": "/blobServices/default/containers/logs/blobs/app.log",
            "eventType": "Microsoft.Storage.BlobCreated",
            "id": "1",
            "data": {}
        }"#;
        let notifications = parse(event_grid_json);
        assert_eq!(notifications.len(), 1);
        let blob_ref = resolve_blob_ref(&notifications[0]).unwrap();
        assert_eq!(blob_ref.storage_account.as_deref(), Some("topicacct"));

        let cloud_event_json = r#"{
            "specversion": "1.0",
            "type": "Microsoft.Storage.BlobCreated",
            "source": "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Storage/storageAccounts/sourceacct",
            "subject": "/blobServices/default/containers/logs/blobs/app.log",
            "id": "1",
            "data": {}
        }"#;
        let notifications = parse(cloud_event_json);
        assert_eq!(notifications.len(), 1);
        let blob_ref = resolve_blob_ref(&notifications[0]).unwrap();
        assert_eq!(blob_ref.storage_account.as_deref(), Some("sourceacct"));
    }

    #[test]
    fn non_retryable_errors_classification() {
        let json_err = serde_json::from_str::<QueueEvent>("bad json").unwrap_err();
        let err1 = ProcessingError::InvalidQueueMessage {
            source: json_err,
            message_id: "1".to_owned(),
        };
        assert!(err1.is_non_retryable());

        let err2 = ProcessingError::InvalidBlobPath {
            subject: None,
            url: None,
        };
        assert!(err2.is_non_retryable());

        let err3 = ProcessingError::ForeignStorageAccount {
            configured: "acct_a".to_owned(),
            received: "acct_b".to_owned(),
        };
        assert!(err3.is_non_retryable());

        let err4 = ProcessingError::ContainerClient {
            message: "err".to_owned(),
            container: "c".to_owned(),
        };
        assert!(!err4.is_non_retryable());
    }
}
