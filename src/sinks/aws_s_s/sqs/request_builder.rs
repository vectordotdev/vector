use vector_common::request_metadata::RequestMetadata;

use crate::event::EventFinalizers;

use super::{config::SqsSinkConfig, MessageBuilder, SendMessageEntry};

#[derive(Clone)]
pub(crate) struct SqsMessageBuilder {
    queue_url: String,
}

impl SqsMessageBuilder {
    pub fn new(config: SqsSinkConfig) -> crate::Result<Self> {
        Ok(Self {
            queue_url: config.queue_url,
        })
    }
}

impl MessageBuilder for SqsMessageBuilder {
    fn build_message(
        &self,
        message_body: String,
        message_group_id: Option<String>,
        message_deduplication_id: Option<String>,
        finalizers: EventFinalizers,
        metadata: RequestMetadata,
    ) -> SendMessageEntry {
        SendMessageEntry {
            message_body,
            message_group_id,
            message_deduplication_id,
            queue_url: self.queue_url.clone(),
            finalizers,
            metadata,
        }
    }
}
