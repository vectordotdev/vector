use crate::config::LogSchema;
use crate::event::{Event, Value};
use chrono::Utc;
use rusoto_logs::InputLogEvent;
use snafu::ResultExt;
use vector_core::event::{EventFinalizers, Finalizable};
use vector_core::ByteSizeOf;

use super::CloudwatchLogsError;
use super::TemplateRenderingFailed;
use crate::sinks::aws_cloudwatch_logs::CloudwatchKey;
use crate::sinks::aws_cloudwatch_logs::IoError;
use crate::sinks::util::encoding::{Encoder, EncodingConfiguration, StandardEncodings};
use crate::sinks::util::processed_event::ProcessedEvent;
use crate::sinks::{
    splunk_hec::common::request::HecRequest,
    util::{encoding::EncodingConfig, Compression, RequestBuilder},
};
use crate::template::Template;

// Estimated maximum size of InputLogEvent with an empty message
const EVENT_SIZE_OVERHEAD: usize = 50;
const MAX_EVENT_SIZE: usize = 256 * 1024;
const MAX_MESSAGE_SIZE: usize = MAX_EVENT_SIZE - EVENT_SIZE_OVERHEAD;

#[derive(Clone)]
pub struct CloudwatchRequest {
    pub key: CloudwatchKey,
    pub message: String,
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
        self.encoding.apply_rules(&mut event);
        let mut message_bytes = vec![];
        self.encoding
            .encode_input(event, &mut message_bytes)
            .context(IoError)?;
        let message = String::from_utf8_lossy(&message_bytes).to_string();

        if message.len() > MAX_MESSAGE_SIZE {
            return Err(CloudwatchLogsError::EventTooLong {
                length: message.len(),
            }
            .into());
        }
        Ok(Some(CloudwatchRequest {
            key,
            message,
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
