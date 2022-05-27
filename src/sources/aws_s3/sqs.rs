use std::{cmp, future::ready, panic, sync::Arc};

use aws_sdk_s3::error::GetObjectError;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_sqs::error::{DeleteMessageBatchError, ReceiveMessageError};
use aws_sdk_sqs::model::{DeleteMessageBatchRequestEntry, Message};
use aws_sdk_sqs::output::DeleteMessageBatchOutput;
use aws_sdk_sqs::Client as SqsClient;
use aws_smithy_client::SdkError;
use aws_types::region::Region;
use bytes::Bytes;
use chrono::{TimeZone, Utc};
use codecs::{decoding::FramingError, CharacterDelimitedDecoder};
use futures::{FutureExt, Stream, StreamExt, TryFutureExt};
use once_cell::sync::Lazy;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use snafu::{ResultExt, Snafu};
use tokio::{pin, select};
use tokio_util::codec::FramedRead;
use tracing::Instrument;
use vector_core::ByteSizeOf;

use crate::tls::TlsConfig;
use crate::{
    config::{log_schema, AcknowledgementsConfig, SourceContext},
    event::{BatchNotifier, BatchStatus, LogEvent},
    internal_events::{
        BytesReceived, OldEventsReceived, SqsMessageDeleteBatchError, SqsMessageDeletePartialError,
        SqsMessageDeleteSucceeded, SqsMessageProcessingError, SqsMessageProcessingSucceeded,
        SqsMessageReceiveError, SqsMessageReceiveSucceeded, SqsS3EventRecordInvalidEventIgnored,
        StreamClosedError,
    },
    line_agg::{self, LineAgg},
    shutdown::ShutdownSignal,
    SourceSender,
};
use lookup::path;

static SUPPORTED_S3S_EVENT_VERSION: Lazy<semver::VersionReq> =
    Lazy::new(|| semver::VersionReq::parse("~2").unwrap());

#[derive(Derivative, Clone, Debug, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub(super) struct Config {
    pub(super) queue_url: String,

    // restricted to u32 for safe conversion to i64 later
    #[serde(default = "default_poll_secs")]
    #[derivative(Default(value = "default_poll_secs()"))]
    pub(super) poll_secs: u32,

    // restricted to u32 for safe conversion to i64 later
    #[serde(default = "default_visibility_timeout_secs")]
    #[derivative(Default(value = "default_visibility_timeout_secs()"))]
    pub(super) visibility_timeout_secs: u32,

    #[serde(default = "default_true")]
    #[derivative(Default(value = "default_true()"))]
    pub(super) delete_message: bool,

    // number of tasks spawned for running the SQS/S3 receive loop
    #[serde(default = "default_client_concurrency")]
    #[derivative(Default(value = "default_client_concurrency()"))]
    pub(super) client_concurrency: u32,

    #[serde(default)]
    #[derivative(Default)]
    pub(super) tls_options: Option<TlsConfig>,
}

const fn default_poll_secs() -> u32 {
    15
}

const fn default_visibility_timeout_secs() -> u32 {
    300
}

const fn default_true() -> bool {
    true
}

fn default_client_concurrency() -> u32 {
    cmp::max(1, num_cpus::get() as u32)
}

#[derive(Debug, Snafu)]
pub(super) enum IngestorNewError {
    #[snafu(display("Invalid visibility timeout {}: {}", timeout, source))]
    InvalidVisibilityTimeout {
        source: std::num::TryFromIntError,
        timeout: u64,
    },
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Snafu)]
pub enum ProcessingError {
    #[snafu(display(
        "Could not parse SQS message with id {} as S3 notification: {}",
        message_id,
        source
    ))]
    InvalidSqsMessage {
        source: serde_json::Error,
        message_id: String,
    },
    #[snafu(display("Failed to fetch s3://{}/{}: {}", bucket, key, source))]
    GetObject {
        source: SdkError<GetObjectError>,
        bucket: String,
        key: String,
    },
    #[snafu(display("Failed to read all of s3://{}/{}: {}", bucket, key, source))]
    ReadObject {
        source: Box<dyn FramingError>,
        bucket: String,
        key: String,
    },
    #[snafu(display("Failed to flush all of s3://{}/{}: {}", bucket, key, source))]
    PipelineSend {
        source: crate::source_sender::ClosedError,
        bucket: String,
        key: String,
    },
    #[snafu(display(
        "Object notification for s3://{}/{} is a bucket in another region: {}",
        bucket,
        key,
        region
    ))]
    WrongRegion {
        region: String,
        bucket: String,
        key: String,
    },
    #[snafu(display("Unsupported S3 event version: {}.", version,))]
    UnsupportedS3EventVersion { version: semver::Version },
    #[snafu(display("Sink reported an error sending events"))]
    ErrorAcknowledgement,
}

pub struct State {
    region: Region,

    s3_client: S3Client,
    sqs_client: SqsClient,

    multiline: Option<line_agg::Config>,
    compression: super::Compression,

    queue_url: String,
    poll_secs: i32,
    client_concurrency: u32,
    visibility_timeout_secs: i32,
    delete_message: bool,
}

pub(super) struct Ingestor {
    state: Arc<State>,
}

impl Ingestor {
    pub(super) async fn new(
        region: Region,
        sqs_client: SqsClient,
        s3_client: S3Client,
        config: Config,
        compression: super::Compression,
        multiline: Option<line_agg::Config>,
    ) -> Result<Ingestor, IngestorNewError> {
        let state = Arc::new(State {
            region,

            s3_client,
            sqs_client,

            compression,
            multiline,

            queue_url: config.queue_url,
            poll_secs: config.poll_secs as i32,
            client_concurrency: config.client_concurrency,
            visibility_timeout_secs: config.visibility_timeout_secs as i32,
            delete_message: config.delete_message,
        });

        Ok(Ingestor { state })
    }

    pub(super) async fn run(
        self,
        cx: SourceContext,
        acknowledgements: AcknowledgementsConfig,
    ) -> Result<(), ()> {
        let acknowledgements = cx.do_acknowledgements(&acknowledgements);
        let mut handles = Vec::new();
        for _ in 0..self.state.client_concurrency {
            let process = IngestorProcess::new(
                Arc::clone(&self.state),
                cx.out.clone(),
                cx.shutdown.clone(),
                acknowledgements,
            );
            let fut = process.run();
            let handle = tokio::spawn(fut.in_current_span());
            handles.push(handle);
        }

        // Wait for all of the processes to finish.  If any one of them panics, we resume
        // that panic here to properly shutdown Vector.
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
}

impl IngestorProcess {
    pub fn new(
        state: Arc<State>,
        out: SourceSender,
        shutdown: ShutdownSignal,
        acknowledgements: bool,
    ) -> Self {
        Self {
            state,
            out,
            shutdown,
            acknowledgements,
        }
    }

    async fn run(mut self) {
        let shutdown = self.shutdown.clone().fuse();
        pin!(shutdown);

        loop {
            select! {
                _ = &mut shutdown => break,
                _ = self.run_once() => {},
            }
        }
    }

    async fn run_once(&mut self) {
        let messages = self.receive_messages().await;
        let messages = messages
            .map(|messages| {
                emit!(SqsMessageReceiveSucceeded {
                    count: messages.len(),
                });
                messages
            })
            .map_err(|err| {
                emit!(SqsMessageReceiveError { error: &err });
                err
            })
            .unwrap_or_default();

        let mut delete_entries = Vec::new();
        for message in messages {
            let receipt_handle = match message.receipt_handle {
                None => {
                    // I don't think this will ever actually happen, but is just an artifact of the
                    // AWS's API predilection for returning nullable values for all response
                    // attributes
                    warn!(message = "Refusing to process message with no receipt_handle.", ?message.message_id);
                    continue;
                }
                Some(ref handle) => handle.to_owned(),
            };

            let message_id = message
                .message_id
                .clone()
                .unwrap_or_else(|| "<unknown>".to_owned());

            match self.handle_sqs_message(message).await {
                Ok(()) => {
                    emit!(SqsMessageProcessingSucceeded {
                        message_id: &message_id
                    });
                    if self.state.delete_message {
                        delete_entries.push(
                            DeleteMessageBatchRequestEntry::builder()
                                .id(message_id)
                                .receipt_handle(receipt_handle)
                                .build(),
                        );
                    }
                }
                Err(err) => {
                    emit!(SqsMessageProcessingError {
                        message_id: &message_id,
                        error: &err,
                    });
                }
            }
        }

        if !delete_entries.is_empty() {
            // We need these for a correct error message if the batch fails overall.
            let cloned_entries = delete_entries.clone();
            match self.delete_messages(delete_entries).await {
                Ok(result) => {
                    // Batch deletes can have partial successes/failures, so we have to check
                    // for both cases and emit accordingly.
                    if let Some(successful_entries) = &result.successful {
                        if !successful_entries.is_empty() {
                            emit!(SqsMessageDeleteSucceeded {
                                message_ids: result.successful.unwrap_or_default(),
                            });
                        }
                    }

                    if let Some(failed_entries) = &result.failed {
                        if !failed_entries.is_empty() {
                            emit!(SqsMessageDeletePartialError {
                                entries: result.failed.unwrap_or_default()
                            });
                        }
                    }
                }
                Err(err) => {
                    emit!(SqsMessageDeleteBatchError {
                        entries: cloned_entries,
                        error: err,
                    });
                }
            }
        }
    }

    async fn handle_sqs_message(&mut self, message: Message) -> Result<(), ProcessingError> {
        let s3_event: S3Event = serde_json::from_str(message.body.unwrap_or_default().as_ref())
            .context(InvalidSqsMessageSnafu {
                message_id: message.message_id.unwrap_or_else(|| "<empty>".to_owned()),
            })?;

        self.handle_s3_event(s3_event).await
    }

    async fn handle_s3_event(&mut self, s3_event: S3Event) -> Result<(), ProcessingError> {
        for record in s3_event.records {
            self.handle_s3_event_record(record).await?
        }
        Ok(())
    }

    async fn handle_s3_event_record(
        &mut self,
        s3_event: S3EventRecord,
    ) -> Result<(), ProcessingError> {
        let event_version: semver::Version = s3_event.event_version.clone().into();
        if !SUPPORTED_S3S_EVENT_VERSION.matches(&event_version) {
            return Err(ProcessingError::UnsupportedS3EventVersion {
                version: event_version.clone(),
            });
        }

        if s3_event.event_name.kind != "ObjectCreated" {
            emit!(SqsS3EventRecordInvalidEventIgnored {
                bucket: &s3_event.s3.bucket.name,
                key: &s3_event.s3.object.key,
                kind: &s3_event.event_name.kind,
                name: &s3_event.event_name.name,
            });
            return Ok(());
        }

        // S3 has to send notifications to a queue in the same region so I don't think this will
        // actually ever be hit unless messages are being forwarded from one queue to another
        if self.state.region.as_ref() != s3_event.aws_region.as_str() {
            return Err(ProcessingError::WrongRegion {
                bucket: s3_event.s3.bucket.name.clone(),
                key: s3_event.s3.object.key.clone(),
                region: s3_event.aws_region,
            });
        }

        let object_result = self
            .state
            .s3_client
            .get_object()
            .bucket(s3_event.s3.bucket.name.clone())
            .key(s3_event.s3.object.key.clone())
            .send()
            .await
            .context(GetObjectSnafu {
                bucket: s3_event.s3.bucket.name.clone(),
                key: s3_event.s3.object.key.clone(),
            });

        let object = object_result?;

        let metadata = object.metadata;
        let timestamp = object
            .last_modified
            .map(|ts| Utc.timestamp(ts.secs(), ts.subsec_nanos()))
            .unwrap_or_else(Utc::now);

        let (batch, receiver) = BatchNotifier::maybe_new_with_receiver(self.acknowledgements);
        let object_reader = super::s3_object_decoder(
            self.state.compression,
            &s3_event.s3.object.key,
            object.content_encoding.as_deref(),
            object.content_type.as_deref(),
            object.body,
        )
        .await;

        // Record the read error seen to propagate up later so we avoid ack'ing the SQS
        // message
        //
        // String is used as we cannot clone std::io::Error to take ownership in closure
        //
        // FramedRead likely stops when it gets an i/o error but I found it more clear to
        // show that we `take_while` there hasn't been an error
        //
        // This can result in objects being partially processed before an error, but we
        // prefer duplicate lines over message loss. Future work could include recording
        // the offset of the object that has been read, but this would only be relevant in
        // the case that the same vector instance processes the same message.
        let mut read_error = None;
        let lines: Box<dyn Stream<Item = Bytes> + Send + Unpin> = Box::new(
            FramedRead::new(object_reader, CharacterDelimitedDecoder::new(b'\n'))
                .map(|res| {
                    res.map(|bytes| {
                        emit!(BytesReceived {
                            byte_size: bytes.len(),
                            protocol: "http",
                        });
                        bytes
                    })
                    .map_err(|err| {
                        read_error = Some(err);
                    })
                    .ok()
                })
                .take_while(|res| ready(res.is_some()))
                .map(|r| r.expect("validated by take_while")),
        );

        let lines = match &self.state.multiline {
            Some(config) => Box::new(
                LineAgg::new(
                    lines.map(|line| ((), line, ())),
                    line_agg::Logic::new(config.clone()),
                )
                .map(|(_src, line, _context)| line),
            ),
            None => lines,
        };

        let bucket_name = Bytes::from(s3_event.s3.bucket.name.as_str().as_bytes().to_vec());
        let object_key = Bytes::from(s3_event.s3.object.key.as_str().as_bytes().to_vec());
        let aws_region = Bytes::from(s3_event.aws_region.as_str().as_bytes().to_vec());

        let mut stream = lines.filter_map(move |line| {
            let mut log = LogEvent::from(line).with_batch_notifier_option(&batch);

            log.insert(path!("bucket"), bucket_name.clone());
            log.insert(path!("object"), object_key.clone());
            log.insert(path!("region"), aws_region.clone());
            log.insert(log_schema().source_type_key(), Bytes::from("aws_s3"));
            log.insert(log_schema().timestamp_key(), timestamp);

            if let Some(metadata) = &metadata {
                for (key, value) in metadata {
                    log.insert(key.as_str(), value.clone());
                }
            }

            emit!(OldEventsReceived {
                count: 1,
                byte_size: log.size_of()
            });

            ready(Some(log))
        });

        let send_error = match self.out.send_event_stream(&mut stream).await {
            Ok(_) => None,
            Err(error) => {
                // count is set to 0 to have no discarded events considering
                // the events are not yet acknowledged and will be retried in
                // case of error
                emit!(StreamClosedError { error, count: 0 });
                Some(crate::source_sender::ClosedError)
            }
        };

        // Up above, `lines` captures `read_error`, and eventually is captured by `stream`,
        // so we explicitly drop it so that we can again utilize `read_error` below.
        drop(stream);

        if let Some(error) = read_error {
            Err(ProcessingError::ReadObject {
                source: error,
                bucket: s3_event.s3.bucket.name.clone(),
                key: s3_event.s3.object.key.clone(),
            })
        } else if let Some(error) = send_error {
            Err(ProcessingError::PipelineSend {
                source: error,
                bucket: s3_event.s3.bucket.name.clone(),
                key: s3_event.s3.object.key.clone(),
            })
        } else {
            match receiver {
                None => Ok(()),
                Some(receiver) => match receiver.await {
                    BatchStatus::Delivered => Ok(()),
                    BatchStatus::Errored => Err(ProcessingError::ErrorAcknowledgement),
                    BatchStatus::Rejected => {
                        error!(
                            message = "Sink reported events were rejected.",
                            internal_log_rate_secs = 5,
                        );
                        // Failed events cannot be retried, so continue to delete the SQS source message.
                        Ok(())
                    }
                },
            }
        }
    }

    async fn receive_messages(&mut self) -> Result<Vec<Message>, SdkError<ReceiveMessageError>> {
        self.state
            .sqs_client
            .receive_message()
            .queue_url(self.state.queue_url.clone())
            .max_number_of_messages(10)
            .visibility_timeout(self.state.visibility_timeout_secs as i32)
            .wait_time_seconds(self.state.poll_secs)
            .send()
            .map_ok(|res| res.messages.unwrap_or_default())
            .await
    }

    async fn delete_messages(
        &mut self,
        entries: Vec<DeleteMessageBatchRequestEntry>,
    ) -> Result<DeleteMessageBatchOutput, SdkError<DeleteMessageBatchError>> {
        self.state
            .sqs_client
            .delete_message_batch()
            .queue_url(self.state.queue_url.clone())
            .set_entries(Some(entries))
            .send()
            .await
    }
}

// https://docs.aws.amazon.com/AmazonS3/latest/dev/notification-content-structure.html
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct S3Event {
    pub records: Vec<S3EventRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct S3EventRecord {
    pub event_version: S3EventVersion,
    pub event_source: String,
    pub aws_region: String,
    pub event_name: S3EventName,

    pub s3: S3Message,
}

#[derive(Clone, Debug)]
pub struct S3EventVersion {
    pub major: u64,
    pub minor: u64,
}

impl From<S3EventVersion> for semver::Version {
    fn from(v: S3EventVersion) -> semver::Version {
        semver::Version::new(v.major, v.minor, 0)
    }
}

// https://docs.aws.amazon.com/AmazonS3/latest/dev/notification-content-structure.html
// <major>.<minor>
impl<'de> Deserialize<'de> for S3EventVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        let s = String::deserialize(deserializer)?;

        let mut parts = s.splitn(2, '.');

        let major = parts
            .next()
            .ok_or_else(|| D::Error::custom("Missing major version number"))?
            .parse::<u64>()
            .map_err(D::Error::custom)?;

        let minor = parts
            .next()
            .ok_or_else(|| D::Error::custom("Missing minor version number"))?
            .parse::<u64>()
            .map_err(D::Error::custom)?;

        Ok(S3EventVersion { major, minor })
    }
}

impl Serialize for S3EventVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{}.{}", self.major, self.minor))
    }
}

#[derive(Clone, Debug)]
pub struct S3EventName {
    pub kind: String,
    pub name: String,
}

// https://docs.aws.amazon.com/AmazonS3/latest/dev/NotificationHowTo.html#supported-notification-event-types
//
// we could use enums here, but that seems overly brittle as deserialization would break if they
// add new event types or names
impl<'de> Deserialize<'de> for S3EventName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        let s = String::deserialize(deserializer)?;

        let mut parts = s.splitn(2, ':');

        let kind = parts
            .next()
            .ok_or_else(|| D::Error::custom("Missing event kind"))?
            .parse::<String>()
            .map_err(D::Error::custom)?;

        let name = parts
            .next()
            .ok_or_else(|| D::Error::custom("Missing event name"))?
            .parse::<String>()
            .map_err(D::Error::custom)?;

        Ok(S3EventName { kind, name })
    }
}

impl Serialize for S3EventName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{}:{}", self.kind, self.name))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct S3Message {
    pub bucket: S3Bucket,
    pub object: S3Object,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct S3Bucket {
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct S3Object {
    // S3ObjectKeys are URL encoded
    // https://docs.aws.amazon.com/AmazonS3/latest/userguide/notification-content-structure.html
    #[serde(with = "urlencoded_string")]
    pub key: String,
}

mod urlencoded_string {
    use percent_encoding::{percent_decode, utf8_percent_encode};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<String, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        use serde::de::Error;

        serde::de::Deserialize::deserialize(deserializer).and_then(|s: &[u8]| {
            let decoded = if s.iter().any(|c| *c == b'+') {
                // AWS encodes spaces as `+` rather than `%20`, so we first need to handle this.
                let s = s
                    .iter()
                    .map(|c| if *c == b'+' { b' ' } else { *c })
                    .collect::<Vec<_>>();
                percent_decode(&s).decode_utf8().map(Into::into)
            } else {
                percent_decode(s).decode_utf8().map(Into::into)
            };

            decoded.map_err(|err| {
                D::Error::custom(format!("error url decoding S3 object key: {}", err))
            })
        })
    }

    pub fn serialize<S>(s: &str, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_str(
            &utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).collect::<String>(),
        )
    }
}

#[test]
fn test_key_deserialize() {
    let value = serde_json::from_str(r#"{"key": "noog+nork"}"#).unwrap();
    assert_eq!(
        S3Object {
            key: "noog nork".to_string(),
        },
        value
    );

    let value = serde_json::from_str(r#"{"key": "noog%2bnork"}"#).unwrap();
    assert_eq!(
        S3Object {
            key: "noog+nork".to_string(),
        },
        value
    );
}
