use bytes::Bytes;
use vector_common::request_metadata::{MetaDescriptive, RequestMetadata};
use vector_core::ByteSizeOf;

use super::config::SqsSinkConfig;
use crate::{
    codecs::{Encoder, Transformer},
    event::{Event, EventFinalizers, Finalizable},
    internal_events::TemplateRenderingError,
    sinks::util::{
        metadata::RequestMetadataBuilder, request_builder::EncodeResult, Compression,
        EncodedLength, RequestBuilder,
    },
    template::Template,
};

#[derive(Clone)]
pub struct SqsMetadata {
    pub finalizers: EventFinalizers,
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
        let serializer = config.encoding.build()?;
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
    type Metadata = SqsMetadata;
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

    fn split_input(
        &self,
        mut event: Event,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
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

        let builder = RequestMetadataBuilder::from_events(&event);

        let sqs_metadata = SqsMetadata {
            finalizers: event.take_finalizers(),
            message_group_id,
            message_deduplication_id,
        };
        (sqs_metadata, builder, event)
    }

    fn build_request(
        &self,
        sqs_metadata: Self::Metadata,
        metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let payload_bytes = payload.into_payload();
        let message_body = String::from(std::str::from_utf8(&payload_bytes).unwrap());

        SendMessageEntry {
            message_body,
            message_group_id: sqs_metadata.message_group_id,
            message_deduplication_id: sqs_metadata.message_deduplication_id,
            queue_url: self.queue_url.clone(),
            finalizers: sqs_metadata.finalizers,
            metadata,
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
    pub metadata: RequestMetadata,
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

impl Finalizable for SendMessageEntry {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}

impl MetaDescriptive for SendMessageEntry {
    fn get_metadata(&self) -> RequestMetadata {
        self.metadata
    }
}
