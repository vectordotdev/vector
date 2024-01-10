use std::num::NonZeroUsize;

use bytes::BytesMut;
use chrono::Utc;
use tokio_util::codec::Encoder as _;
use vector_lib::request_metadata::{MetaDescriptive, RequestMetadata};
use vector_lib::{
    event::{EventFinalizers, Finalizable},
    ByteSizeOf,
};

use super::TemplateRenderingError;
use crate::{
    codecs::{Encoder, Transformer},
    event::{Event, Value},
    internal_events::AwsCloudwatchLogsMessageSizeError,
    sinks::{aws_cloudwatch_logs::CloudwatchKey, util::metadata::RequestMetadataBuilder},
    template::Template,
};

// Estimated maximum size of InputLogEvent with an empty message
const EVENT_SIZE_OVERHEAD: usize = 50;
const MAX_EVENT_SIZE: usize = 256 * 1024;
const MAX_MESSAGE_SIZE: usize = MAX_EVENT_SIZE - EVENT_SIZE_OVERHEAD;

#[derive(Clone)]
pub struct CloudwatchRequest {
    pub key: CloudwatchKey,
    pub(super) message: String,
    pub timestamp: i64,
    pub finalizers: EventFinalizers,
    metadata: RequestMetadata,
}

impl Finalizable for CloudwatchRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}

impl MetaDescriptive for CloudwatchRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

pub struct CloudwatchRequestBuilder {
    pub group_template: Template,
    pub stream_template: Template,
    pub transformer: Transformer,
    pub encoder: Encoder<()>,
}

impl CloudwatchRequestBuilder {
    pub fn build(&mut self, mut event: Event) -> Option<CloudwatchRequest> {
        let group = match self.group_template.render_string(&event) {
            Ok(b) => b,
            Err(error) => {
                emit!(TemplateRenderingError {
                    error,
                    field: Some("group"),
                    drop_event: true,
                });
                return None;
            }
        };

        let stream = match self.stream_template.render_string(&event) {
            Ok(b) => b,
            Err(error) => {
                emit!(TemplateRenderingError {
                    error,
                    field: Some("stream"),
                    drop_event: true,
                });
                return None;
            }
        };
        let key = CloudwatchKey { group, stream };

        let timestamp = match event.as_mut_log().remove_timestamp() {
            Some(Value::Timestamp(ts)) => ts.timestamp_millis(),
            _ => Utc::now().timestamp_millis(),
        };

        let finalizers = event.take_finalizers();
        self.transformer.transform(&mut event);
        let mut message_bytes = BytesMut::new();

        let builder = RequestMetadataBuilder::from_event(&event);

        if self.encoder.encode(event, &mut message_bytes).is_err() {
            // The encoder handles internal event emission for Error and EventsDropped.
            return None;
        }
        let message = String::from_utf8_lossy(&message_bytes).to_string();

        if message.len() > MAX_MESSAGE_SIZE {
            emit!(AwsCloudwatchLogsMessageSizeError {
                size: message.len(),
                max_size: MAX_MESSAGE_SIZE,
            });
            return None;
        }

        let bytes_len =
            NonZeroUsize::new(message_bytes.len()).expect("payload should never be zero length");
        let metadata = builder.with_request_size(bytes_len);

        Some(CloudwatchRequest {
            key,
            message,
            timestamp,
            finalizers,
            metadata,
        })
    }
}

/// ByteSizeOf is being abused to represent the encoded size of a request for the Partitioned Batcher
///
/// The maximum batch size is 1,048,576 bytes. This size is calculated as the sum of all event messages in UTF-8, plus 26 bytes for each log event.
/// source: <https://docs.aws.amazon.com/AmazonCloudWatchLogs/latest/APIReference/API_PutLogEvents.html>
impl ByteSizeOf for CloudwatchRequest {
    fn size_of(&self) -> usize {
        self.message.len() + 26
    }

    fn allocated_bytes(&self) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use vector_lib::config::log_schema;
    use vector_lib::event::LogEvent;

    use super::CloudwatchRequestBuilder;

    #[test]
    fn test() {
        let mut request_builder = CloudwatchRequestBuilder {
            group_template: "group".try_into().unwrap(),
            stream_template: "stream".try_into().unwrap(),
            transformer: Default::default(),
            encoder: Default::default(),
        };
        let timestamp = Utc::now();
        let message = "event message";
        let mut event = LogEvent::from(message);
        event.insert(log_schema().timestamp_key_target_path().unwrap(), timestamp);

        let request = request_builder.build(event.into()).unwrap();
        assert_eq!(request.timestamp, timestamp.timestamp_millis());
        assert_eq!(&request.message, message);
    }
}
