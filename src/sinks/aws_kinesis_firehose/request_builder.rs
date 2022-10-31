use std::io;

use aws_sdk_firehose::{model::Record, types::Blob};
use bytes::Bytes;
use vector_common::request_metadata::{MetaDescriptive, RequestMetadata};
use vector_core::ByteSizeOf;

use crate::{
    codecs::{Encoder, Transformer},
    event::{Event, EventFinalizers, Finalizable},
    sinks::util::{
        metadata::RequestMetadataBuilder, request_builder::EncodeResult, Compression,
        RequestBuilder,
    },
};

use super::sink::{KinesisKey, KinesisProcessedEvent};

pub struct KinesisRequestBuilder {
    pub(super) compression: Compression,
    pub(super) encoder: (Transformer, Encoder<()>),
}

pub struct KinesisMetadata {
    pub finalizers: EventFinalizers,
    pub partition_key: String,
}

#[derive(Clone)]
pub struct KinesisRequest {
    pub key: KinesisKey,
    pub record: Record,
    pub finalizers: EventFinalizers,
    metadata: RequestMetadata,
}

impl Finalizable for KinesisRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for KinesisRequest {
    fn get_metadata(&self) -> RequestMetadata {
        self.metadata
    }
}

impl KinesisRequest {
    fn encoded_length(&self) -> usize {
        let data_len = self
            .record
            .data
            .as_ref()
            .map(|x| x.as_ref().len())
            .unwrap_or(0);
        // data is simply base64 encoded, quoted, and comma separated
        (data_len + 2) / 3 * 4 + 3
    }
}

impl ByteSizeOf for KinesisRequest {
    fn size_of(&self) -> usize {
        // `ByteSizeOf` is being somewhat abused here. This is
        // used by the batcher. `encoded_length` is needed so that final
        // batched size doesn't exceed the Firehose limits
        self.encoded_length()
    }

    fn allocated_bytes(&self) -> usize {
        0
    }
}

impl RequestBuilder<KinesisProcessedEvent> for KinesisRequestBuilder {
    type Metadata = KinesisMetadata;
    type Events = Event;
    type Encoder = (Transformer, Encoder<()>);
    type Payload = Bytes;
    type Request = KinesisRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        mut event: KinesisProcessedEvent,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let kinesis_metadata = KinesisMetadata {
            finalizers: event.take_finalizers(),
            partition_key: event.metadata.partition_key,
        };
        let event = Event::from(event.event);
        let builder = RequestMetadataBuilder::from_events(&event);

        (kinesis_metadata, builder, event)
    }

    fn build_request(
        &self,
        kinesis_metadata: Self::Metadata,
        metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let payload_bytes = payload.into_payload();

        KinesisRequest {
            key: KinesisKey {
                partition_key: kinesis_metadata.partition_key,
            },
            record: Record::builder()
                .data(Blob::new(&payload_bytes[..]))
                .build(),
            finalizers: kinesis_metadata.finalizers,
            metadata,
        }
    }
}
