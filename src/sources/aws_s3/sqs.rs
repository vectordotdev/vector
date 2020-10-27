use crate::{
    config::log_schema,
    event::Event,
    internal_events::aws_s3::source::{
        SqsMessageDeleteFailed, SqsMessageDeleteSucceeded, SqsMessageProcessingFailed,
        SqsMessageProcessingSucceeded, SqsMessageReceiveFailed, SqsMessageReceiveSucceeded,
        SqsS3EventRecordIgnoredInvalidEvent,
    },
    line_agg::{self, LineAgg},
    shutdown::ShutdownSignal,
    Pipeline,
};
use bytes::Bytes;
use chrono::{DateTime, TimeZone, Utc};
use codec::BytesDelimitedCodec;
use futures::{
    compat::{Compat, Future01CompatExt},
    future::TryFutureExt,
    stream::{Stream, StreamExt},
};
use futures01::Sink;
use rusoto_core::{Region, RusotoError};
use rusoto_s3::{GetObjectError, GetObjectRequest, S3Client, S3};
use rusoto_sqs::{
    DeleteMessageError, DeleteMessageRequest, GetQueueUrlError, GetQueueUrlRequest, Message,
    ReceiveMessageError, ReceiveMessageRequest, Sqs, SqsClient,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use snafu::{ResultExt, Snafu};
use std::{convert::TryInto, time::Duration};
use tokio::{select, time};
use tokio_util::codec::FramedRead;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub queue_name: String,
    pub queue_owner: Option<String>,
    #[serde(default = "default_poll_interval_secs")]
    pub poll_secs: u64,
    #[serde(default = "default_visibility_timeout_secs")]
    pub visibility_timeout_secs: u64,
    #[serde(default = "default_true")]
    pub delete_message: bool,
}

const fn default_poll_interval_secs() -> u64 {
    15
}

const fn default_visibility_timeout_secs() -> u64 {
    300
}
const fn default_true() -> bool {
    true
}

#[derive(Debug, Snafu)]
pub enum IngestorNewError {
    #[snafu(display("Unable to fetch queue URL for {}: {}", name, source))]
    FetchQueueUrl {
        source: RusotoError<GetQueueUrlError>,
        name: String,
        owner: Option<String>,
    },
    #[snafu(display("Got an empty queue URL for {}", name))]
    MissingQueueUrl { name: String, owner: Option<String> },
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
    #[snafu(display("Failed to read all of s3://{}/{}: {}", bucket, key, error))]
    ReadObject {
        error: String,
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
}

pub struct Ingestor {
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
    pub async fn new(
        region: Region,
        sqs_client: SqsClient,
        s3_client: S3Client,
        config: Config,
        compression: super::Compression,
        multiline: Option<line_agg::Config>,
    ) -> Result<Ingestor, IngestorNewError> {
        let queue_url_result = sqs_client
            .get_queue_url(GetQueueUrlRequest {
                queue_name: config.queue_name.clone(),
                queue_owner_aws_account_id: config.queue_owner.clone(),
                ..Default::default()
            })
            .await
            .with_context(|| FetchQueueUrl {
                name: config.queue_name.clone(),
                owner: config.queue_owner.clone(),
            })?;

        let queue_url = queue_url_result
            .queue_url
            .ok_or(IngestorNewError::MissingQueueUrl {
                name: config.queue_name.clone(),
                owner: config.queue_owner.clone(),
            })?;

        // This is a bit odd as AWS wants an i64 for this value, but also doesn't want negative
        // values so I used u64 for the config deserialization and validate that there is no
        // overflow here
        let visibility_timeout_secs: i64 =
            config
                .visibility_timeout_secs
                .try_into()
                .context(InvalidVisibilityTimeout {
                    timeout: config.visibility_timeout_secs,
                })?;

        Ok(Ingestor {
            region,

            s3_client,
            sqs_client,

            compression,
            multiline,

            queue_url,
            poll_interval: Duration::from_secs(config.poll_secs),
            visibility_timeout_secs,
            delete_message: config.delete_message,
        })
    }

    pub async fn run(self, out: Pipeline, mut shutdown: ShutdownSignal) -> Result<(), ()> {
        let mut interval = time::interval(self.poll_interval).map(|_| ());

        loop {
            select! {
                Some(()) = interval.next() => (),
                _ = &mut shutdown => break Ok(()),
                else => break Ok(()),
            };

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

                let message_id = message.message_id.clone().unwrap_or("<unknown>".to_owned());

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
    }

    async fn handle_sqs_message(
        &self,
        message: Message,
        out: Pipeline,
    ) -> Result<(), ProcessingError> {
        let s3_event: S3Event = serde_json::from_str(message.body.unwrap_or_default().as_ref())
            .context(InvalidSqsMessage {
                message_id: message.message_id.unwrap_or("<empty>".to_owned()),
            })?;

        self.handle_s3_event(s3_event, out).await
    }

    async fn handle_s3_event(
        &self,
        s3_event: S3Event,
        out: Pipeline,
    ) -> Result<(), ProcessingError> {
        for record in s3_event.records {
            self.handle_s3_event_record(record, out.clone()).await?
        }
        Ok(())
    }

    async fn handle_s3_event_record(
        &self,
        s3_event: S3EventRecord,
        out: Pipeline,
    ) -> Result<(), ProcessingError> {
        if s3_event.event_name.kind != "ObjectCreated" {
            emit!(SqsS3EventRecordIgnoredInvalidEvent {
                bucket: &s3_event.s3.bucket.name,
                key: &s3_event.s3.object.key,
                kind: &s3_event.event_name.kind,
                name: &s3_event.event_name.name,
            });
            return Ok(());
        }

        // S3 has to send notifications to a queue in the same region so I don't think this will
        // actually ever be it unless messages are being forwarded from one queue to another
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
                let r = super::s3_object_decoder(
                    self.compression,
                    &s3_event.s3.object.key,
                    object.content_encoding.as_deref(),
                    object.content_type.as_deref(),
                    body,
                );

                // Record the read error saw to propagate up later so we avoid ack'ing the SQS
                // message
                //
                // String is used as we cannot take clone std::io::Error to take ownership in
                // closure
                //
                // FramedRead likely stops when it gets an i/o error but I found it more clear to
                // show that we `take_while` there hasn't been an error
                //
                // This can result in objects being partially processed before an error, but we
                // prefer duplicate lines over message loss. Future work could include recording
                // the offset of the object that has been read, but this would only be relevant in
                // the case that the same vector instance processes the same message.
                let mut read_error: Option<String> = None;
                let mut lines: Box<dyn Stream<Item = Bytes> + Send + Unpin> = Box::new(
                    FramedRead::new(r, BytesDelimitedCodec::new(b'\n'))
                        .take_while(|r| {
                            futures::future::ready(match r {
                                Ok(_) => true,
                                Err(err) => {
                                    read_error = Some(err.to_string());
                                    false
                                }
                            })
                        })
                        .map(|r| r.unwrap()), // validated by take_while
                );
                if let Some(config) = &self.multiline {
                    lines = Box::new(
                        LineAgg::new(
                            lines.map(|line| ((), line, ())),
                            line_agg::Logic::new(config.clone()),
                        )
                        .map(|(_src, line, _context)| line),
                    );
                }

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

                    futures::future::ready(Some(Ok(event)))
                });

                out.send_all(Compat::new(Box::pin(stream)))
                    .compat()
                    .await
                    .map_err(|error| {
                        error!(message = "Error sending S3 Logs", %error);
                    })
                    .ok();

                read_error
                    .map(|error| {
                        Err(ProcessingError::ReadObject {
                            error,
                            bucket: s3_event.s3.bucket.name.clone(),
                            key: s3_event.s3.object.key.clone(),
                        })
                    })
                    .unwrap_or(Ok(()))
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
                ..Default::default()
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
    event_version: String, // TODO compare >=
    event_source: String,
    aws_region: String,
    event_name: S3EventName,

    s3: S3Message,
}

#[derive(Clone, Debug)]
struct S3EventName {
    kind: String,
    name: String,
}

// https://docs.aws.amazon.com/AmazonS3/latest/dev/NotificationHowTo.html#supported-notification-event-types
//
// we could enums here, but that seems overly brittle as deserialization would  break if they add
// new event types or names
impl<'de> Deserialize<'de> for S3EventName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        let s = String::deserialize(deserializer)?;

        let mut parts = s.splitn(2, ":");

        let kind = parts
            .next()
            .ok_or(D::Error::custom("Missing event type"))?
            .parse::<String>()
            .map_err(D::Error::custom)?;

        let name = parts
            .next()
            .ok_or(D::Error::custom("Missing event name"))?
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
