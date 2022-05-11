use bytes::Bytes;
use vector_core::buffers::Ackable;
use vector_core::ByteSizeOf;

use super::config::SqsSinkConfig;
use crate::codecs::Encoder;
use crate::event::{Event, EventFinalizers, Finalizable};
use crate::internal_events::TemplateRenderingError;
use crate::sinks::util::encoding::Transformer;
use crate::sinks::util::{Compression, EncodedLength, RequestBuilder};
use crate::template::Template;

#[derive(Clone)]
pub struct Metadata {
    pub finalizers: EventFinalizers,
    pub event_byte_size: usize,
    pub message_group_id: Option<String>,
    pub message_deduplication_id: Option<String>,
}

#[derive(Clone)]
pub(crate) struct SqsRequestBuilder {
    encoder: (Transformer, Encoder<()>),
    message_group_id: Option<Template>,
    message_deduplication_id: Option<Template>,
    queue_url: String,
}

impl SqsRequestBuilder {
    pub fn new(config: SqsSinkConfig) -> crate::Result<Self> {
        let transformer = config.encoding.transformer();
        let serializer = config.encoding.encoding();
        let encoder = Encoder::<()>::new(serializer);

        Ok(Self {
            encoder: (transformer, encoder),
            message_group_id: config.message_group_id()?,
            message_deduplication_id: config.message_deduplication_id()?,
            queue_url: config.queue_url,
        })
    }
}

impl RequestBuilder<Event> for SqsRequestBuilder {
    type Metadata = Metadata;
    type Events = Event;
    type Encoder = (Transformer, Encoder<()>);
    type Payload = Bytes;
    type Request = SendMessageEntry;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        Compression::None
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(&self, mut event: Event) -> (Self::Metadata, Self::Events) {
        let event_byte_size = event.size_of();

        let message_group_id = match self.message_group_id {
            Some(ref tpl) => match tpl.render_string(&event) {
                Ok(value) => Some(value),
                Err(error) => {
                    emit!(TemplateRenderingError {
                        error,
                        field: Some("message_group_id"),
                        drop_event: true,
                    });
                    None
                }
            },
            None => None,
        };
        let message_deduplication_id = match self.message_deduplication_id {
            Some(ref tpl) => match tpl.render_string(&event) {
                Ok(value) => Some(value),
                Err(error) => {
                    emit!(TemplateRenderingError {
                        error,
                        field: Some("message_deduplication_id"),
                        drop_event: true,
                    });
                    None
                }
            },
            None => None,
        };

        let metadata = Metadata {
            finalizers: event.take_finalizers(),
            event_byte_size,
            message_group_id,
            message_deduplication_id,
        };
        (metadata, event)
    }

    fn build_request(&self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let message_body = String::from(std::str::from_utf8(&payload).unwrap());
        SendMessageEntry {
            message_body,
            message_group_id: metadata.message_group_id,
            message_deduplication_id: metadata.message_deduplication_id,
            queue_url: self.queue_url.clone(),
            finalizers: metadata.finalizers,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SendMessageEntry {
    pub message_body: String,
    pub message_group_id: Option<String>,
    pub message_deduplication_id: Option<String>,
    pub queue_url: String,
    finalizers: EventFinalizers,
}

impl ByteSizeOf for SendMessageEntry {
    fn allocated_bytes(&self) -> usize {
        self.message_body.size_of()
            + self.message_group_id.size_of()
            + self.message_deduplication_id.size_of()
    }
}

impl EncodedLength for SendMessageEntry {
    fn encoded_length(&self) -> usize {
        self.message_body.len()
    }
}

impl Ackable for SendMessageEntry {
    fn ack_size(&self) -> usize {
        1
    }
}

impl Finalizable for SendMessageEntry {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}
