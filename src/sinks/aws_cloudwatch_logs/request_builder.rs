use chrono::Utc;
use snafu::ResultExt;
use vector_core::{
    event::{EventFinalizers, Finalizable},
    ByteSizeOf,
};

use super::{CloudwatchLogsError, TemplateRenderingFailed};
use crate::{
    config::LogSchema,
    event::{Event, Value},
    sinks::{
        aws_cloudwatch_logs::{CloudwatchKey, IoError},
        util::encoding::{Encoder, EncodingConfig, EncodingConfiguration, StandardEncodings},
    },
    template::Template,
};

// Estimated maximum size of InputLogEvent with an empty message
const EVENT_SIZE_OVERHEAD: usize = 50;
const MAX_EVENT_SIZE: usize = 256 * 1024;
const MAX_MESSAGE_SIZE: usize = MAX_EVENT_SIZE - EVENT_SIZE_OVERHEAD;

#[derive(Clone)]
pub struct CloudwatchRequest {
    pub key: CloudwatchKey,
    pub message: String,
    pub event_byte_size: usize,
    pub timestamp: i64,
    pub finalizers: EventFinalizers,
}

impl Finalizable for CloudwatchRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}

pub struct CloudwatchRequestBuilder {
    pub group_template: Template,
    pub stream_template: Template,
    pub log_schema: LogSchema,
    pub encoding: EncodingConfig<StandardEncodings>,
}

impl CloudwatchRequestBuilder {
    pub fn build(
        &self,
        mut event: Event,
    ) -> Result<Option<CloudwatchRequest>, CloudwatchLogsError> {
        let group = match self.group_template.render_string(&event) {
            Ok(b) => b,
            Err(error) => {
                emit!(&TemplateRenderingFailed {
                    error,
                    field: Some("group"),
                    drop_event: true,
                });
                return Ok(None);
            }
        };

        let stream = match self.stream_template.render_string(&event) {
            Ok(b) => b,
            Err(error) => {
                emit!(&TemplateRenderingFailed {
                    error,
                    field: Some("stream"),
                    drop_event: true,
                });
                return Ok(None);
            }
        };
        let key = CloudwatchKey { group, stream };

        let timestamp = match event.as_mut_log().remove(self.log_schema.timestamp_key()) {
            Some(Value::Timestamp(ts)) => ts.timestamp_millis(),
            _ => Utc::now().timestamp_millis(),
        };

        let finalizers = event.take_finalizers();
        let event_byte_size = event.size_of();
        self.encoding.apply_rules(&mut event);
        let mut message_bytes = vec![];
        self.encoding
            .encode_input(event, &mut message_bytes)
            .context(IoError)?;
        let message = String::from_utf8_lossy(&message_bytes).to_string();

        if message.len() > MAX_MESSAGE_SIZE {
            return Err(CloudwatchLogsError::EventTooLong {
                length: message.len(),
            });
        }
        Ok(Some(CloudwatchRequest {
            key,
            message,
            event_byte_size,
            timestamp,
            finalizers,
        }))
    }
}

/// ByteSizeOf is being abused to represent the encoded size of a request for the Partitioned Batcher
///
/// The maximum batch size is 1,048,576 bytes. This size is calculated as the sum of all event messages in UTF-8, plus 26 bytes for each log event.
/// source: https://docs.aws.amazon.com/AmazonCloudWatchLogs/latest/APIReference/API_PutLogEvents.html
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
    use super::*;
    use crate::config::log_schema;

    #[test]
    fn test() {
        let request_builder = CloudwatchRequestBuilder {
            group_template: "group".try_into().unwrap(),
            stream_template: "stream".try_into().unwrap(),
            log_schema: log_schema().clone(),
            encoding: EncodingConfig::from(StandardEncodings::Text),
        };
        let timestamp = Utc::now();
        let message = "event message";
        let mut event = Event::from(message);
        event
            .as_mut_log()
            .insert(log_schema().timestamp_key(), timestamp);

        let request = request_builder.build(event).unwrap().unwrap();
        assert_eq!(request.timestamp, timestamp.timestamp_millis());
        assert_eq!(&request.message, message);
    }
}
