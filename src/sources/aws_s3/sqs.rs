use crate::{
    config::log_schema,
    event::Event,
    internal_events::aws_s3::source::{
        SqsMessageDeleteFailed, SqsMessageDeleteSucceeded, SqsMessageProcessingFailed,
        SqsMessageProcessingSucceeded, SqsMessageReceiveFailed, SqsMessageReceiveSucceeded,
        SqsS3EventRecordInvalidEventIgnored,
    },
    line_agg::{self, LineAgg},
    shutdown::ShutdownSignal,
    Pipeline,
};
use bytes::Bytes;
use chrono::{DateTime, TimeZone, Utc};
use codec::BytesDelimitedCodec;
use futures::{SinkExt, Stream, StreamExt, TryFutureExt};
use lazy_static::lazy_static;
use rusoto_core::{Region, RusotoError};
use rusoto_s3::{GetObjectError, GetObjectRequest, S3Client, S3};
use rusoto_sqs::{
    DeleteMessageError, DeleteMessageRequest, Message, ReceiveMessageError, ReceiveMessageRequest,
    Sqs, SqsClient,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use snafu::{ResultExt, Snafu};
use std::{future::ready, time::Duration};
use tokio::time;
use tokio_util::codec::FramedRead;

lazy_static! {
    static ref SUPPORTED_S3S_EVENT_VERSION: semver::VersionReq =
        semver::VersionReq::parse("~2").unwrap();
}

#[derive(Derivative, Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Config {
    pub(super) queue_url: String,

    #[serde(default = "default_poll_interval_secs")]
    #[derivative(Default(value = "default_poll_interval_secs()"))]
    pub(super) poll_secs: u64,
    #[serde(default = "default_visibility_timeout_secs")]
    #[derivative(Default(value = "default_visibility_timeout_secs()"))]
    // restricted to u32 for safe conversion to i64 later
    pub(super) visibility_timeout_secs: u32,
    #[serde(default = "default_true")]
    #[derivative(Default(value = "default_true()"))]
    pub(super) delete_message: bool,
}

const fn default_poll_interval_secs() -> u64 {
    15
}

const fn default_visibility_timeout_secs() -> u32 {
    300
}
const fn default_true() -> bool {
    true
}

#[derive(Debug, Snafu)]
pub(super) enum IngestorNewError {
    #[snafu(display("Invalid visibility timeout {}: {}", timeout, source))]
    InvalidVisibilityTimeout {
        source: std::num::TryFromIntError,
        timeout: u64,
    },
}

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
        source: RusotoError<GetObjectError>,
        bucket: String,
        key: String,
    },
    #[snafu(display("Failed to read all of s3://{}/{}: {}", bucket, key, source))]
    ReadObject {
        source: std::io::Error,
        bucket: String,
        key: String,
    },
    #[snafu(display("Failed to flush all of s3://{}/{}: {}", bucket, key, source))]
    PipelineSend {
        source: crate::pipeline::ClosedError,
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
}

pub(super) struct Ingestor {
    region: Region,

    s3_client: S3Client,
    sqs_client: SqsClient,

    multiline: Option<line_agg::Config>,
    compression: super::Compression,

    queue_url: String,
    poll_interval: Duration,
    visibility_timeout_secs: i64,
    delete_message: bool,
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
        let visibility_timeout_secs: i64 = config.visibility_timeout_secs.into();

        Ok(Ingestor {
            region,

            s3_client,
            sqs_client,

            compression,
            multiline,

            queue_url: config.queue_url,
            poll_interval: Duration::from_secs(config.poll_secs),
            visibility_timeout_secs,
            delete_message: config.delete_message,
        })
    }

    pub(super) async fn run(self, out: Pipeline, shutdown: ShutdownSignal) -> Result<(), ()> {
        time::interval(self.poll_interval)
            .take_until(shutdown)
            .for_each(|_| self.run_once(&out))
            .await;

        Ok(())
    }

    async fn run_once(&self, out: &Pipeline) {
        let messages = self
            .receive_messages()
            .inspect_ok(|messages| {
                emit!(SqsMessageReceiveSucceeded {
                    count: messages.len(),
                });
            })
            .inspect_err(|err| {
                emit!(SqsMessageReceiveFailed { error: err });
            })
            .await
            .unwrap_or_default();

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

            match self.handle_sqs_message(message, out.clone()).await {
                Ok(()) => {
                    emit!(SqsMessageProcessingSucceeded {
                        message_id: &message_id
                    });
                    if self.delete_message {
                        match self.delete_message(receipt_handle).await {
                            Ok(_) => {
                                emit!(SqsMessageDeleteSucceeded {
                                    message_id: &message_id
                                });
                            }
                            Err(err) => {
                                emit!(SqsMessageDeleteFailed {
                                    error: &err,
                                    message_id: &message_id,
                                });
                            }
                        }
                    }
                }
                Err(err) => {
                    emit!(SqsMessageProcessingFailed {
                        message_id: &message_id,
                        error: &err,
                    });
                }
            }
        }
    }

    async fn handle_sqs_message(
        &self,
        message: Message,
        out: Pipeline,
    ) -> Result<(), ProcessingError> {
        let s3_event: S3Event = serde_json::from_str(message.body.unwrap_or_default().as_ref())
            .context(InvalidSqsMessage {
                message_id: message.message_id.unwrap_or_else(|| "<empty>".to_owned()),
            })?;

        self.handle_s3_event(s3_event, out).await
    }

    async fn handle_s3_event(
        &self,
        s3_event: S3Event,
        mut out: Pipeline,
    ) -> Result<(), ProcessingError> {
        for record in s3_event.records {
            self.handle_s3_event_record(record, &mut out).await?
        }
        Ok(())
    }

    async fn handle_s3_event_record(
        &self,
        s3_event: S3EventRecord,
        out: &mut Pipeline,
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
        if self.region.name() != s3_event.aws_region {
            return Err(ProcessingError::WrongRegion {
                bucket: s3_event.s3.bucket.name.clone(),
                key: s3_event.s3.object.key.clone(),
                region: s3_event.aws_region,
            });
        }

        let object = self
            .s3_client
            .get_object(GetObjectRequest {
                bucket: s3_event.s3.bucket.name.clone(),
                key: s3_event.s3.object.key.clone(),
                ..Default::default()
            })
            .await
            .context(GetObject {
                bucket: s3_event.s3.bucket.name.clone(),
                key: s3_event.s3.object.key.clone(),
            })?;

        let metadata = object.metadata;
        let timestamp = object
            .last_modified
            .and_then(|t| {
                DateTime::parse_from_rfc2822(&t)
                    .map(|ts| Utc.timestamp(ts.timestamp(), ts.timestamp_subsec_nanos()))
                    .ok()
            })
            .unwrap_or_else(Utc::now);

        match object.body {
            Some(body) => {
                let object_reader = super::s3_object_decoder(
                    self.compression,
                    &s3_event.s3.object.key,
                    object.content_encoding.as_deref(),
                    object.content_type.as_deref(),
                    body,
                );

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
                let mut read_error: Option<std::io::Error> = None;
                let lines: Box<dyn Stream<Item = Bytes> + Send + Unpin> = Box::new(
                    FramedRead::new(object_reader, BytesDelimitedCodec::new(b'\n'))
                        .map(|res| {
                            res.map_err(|err| {
                                read_error = Some(err);
                            })
                            .ok()
                        })
                        .take_while(|res| ready(res.is_some()))
                        .map(|r| r.expect("validated by take_while")),
                );

                let lines = match &self.multiline {
                    Some(config) => Box::new(
                        LineAgg::new(
                            lines.map(|line| ((), line, ())),
                            line_agg::Logic::new(config.clone()),
                        )
                        .map(|(_src, line, _context)| line),
                    ),
                    None => lines,
                };

                let stream = lines.filter_map(|line| {
                    let mut event = Event::from(line);

                    let log = event.as_mut_log();
                    log.insert("bucket", s3_event.s3.bucket.name.clone());
                    log.insert("object", s3_event.s3.object.key.clone());
                    log.insert("region", s3_event.aws_region.clone());
                    log.insert(log_schema().timestamp_key(), timestamp);

                    if let Some(metadata) = &metadata {
                        for (key, value) in metadata {
                            log.insert(key, value.clone());
                        }
                    }

                    ready(Some(Ok(event)))
                });

                let mut send_error: Option<crate::pipeline::ClosedError> = None;
                out.send_all(&mut Box::pin(stream))
                    .await
                    .map_err(|err| {
                        send_error = Some(err);
                    })
                    .ok();

                read_error
                    .map(|error| {
                        Err(ProcessingError::ReadObject {
                            source: error,
                            bucket: s3_event.s3.bucket.name.clone(),
                            key: s3_event.s3.object.key.clone(),
                        })
                    })
                    .unwrap_or_else(|| {
                        send_error
                            .map(|error| {
                                Err(ProcessingError::PipelineSend {
                                    source: error,
                                    bucket: s3_event.s3.bucket.name.clone(),
                                    key: s3_event.s3.object.key.clone(),
                                })
                            })
                            .unwrap_or(Ok(()))
                    })
            }
            None => Ok(()),
        }
    }

    async fn receive_messages(&self) -> Result<Vec<Message>, RusotoError<ReceiveMessageError>> {
        self.sqs_client
            .receive_message(ReceiveMessageRequest {
                queue_url: self.queue_url.clone(),
                max_number_of_messages: Some(10),
                visibility_timeout: Some(self.visibility_timeout_secs),
                ..Default::default()
            })
            .map_ok(|res| res.messages.unwrap_or_default())
            .await
    }

    async fn delete_message(
        &self,
        receipt_handle: String,
    ) -> Result<(), RusotoError<DeleteMessageError>> {
        self.sqs_client
            .delete_message(DeleteMessageRequest {
                queue_url: self.queue_url.clone(),
                receipt_handle,
            })
            .await
    }
}

// https://docs.aws.amazon.com/AmazonS3/latest/dev/notification-content-structure.html
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
struct S3Event {
    records: Vec<S3EventRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct S3EventRecord {
    event_version: S3EventVersion,
    event_source: String,
    aws_region: String,
    event_name: S3EventName,

    s3: S3Message,
}

#[derive(Clone, Debug)]
struct S3EventVersion {
    major: u64,
    minor: u64,
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
struct S3EventName {
    kind: String,
    name: String,
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
        serializer.serialize_str(&format!("{}:{}", self.name, self.kind))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct S3Message {
    bucket: S3Bucket,
    object: S3Object,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct S3Bucket {
    name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct S3Object {
    key: String,
}
