use std::io;

use aws_sdk_kinesis::{model::PutRecordsRequestEntry, types::Blob};
use bytes::Bytes;
use vector_common::request_metadata::{MetaDescriptive, RequestMetadata};
use vector_core::ByteSizeOf;

use super::sink::{KinesisKey, KinesisProcessedEvent};
use crate::{
    codecs::{Encoder, Transformer},
    event::{Event, EventFinalizers, Finalizable},
    sinks::util::{
        metadata::RequestMetadataBuilder, request_builder::EncodeResult, Compression,
        RequestBuilder,
    },
};

pub struct KinesisRequestBuilder {
    pub compression: Compression,
    pub encoder: (Transformer, Encoder<()>),
}

pub struct KinesisMetadata {
    pub finalizers: EventFinalizers,
    pub partition_key: String,
}

#[derive(Clone)]
pub struct KinesisRequest {
    pub key: KinesisKey,
    pub put_records_request: PutRecordsRequestEntry,
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
        let hash_key_size = self
            .put_records_request
            .explicit_hash_key
            .as_ref()
            .map(|s| s.len())
            .unwrap_or_default();

        // data is base64 encoded
        let data_len = self
            .put_records_request
            .data
            .as_ref()
            .map(|data| data.as_ref().len())
            .unwrap_or(0);

        let key_len = self
            .put_records_request
            .partition_key
            .as_ref()
            .map(|key| key.len())
            .unwrap_or(0);

        (data_len + 2) / 3 * 4 + hash_key_size + key_len + 10
    }
}

impl ByteSizeOf for KinesisRequest {
    fn size_of(&self) -> usize {
        // `ByteSizeOf` is being somewhat abused here. This is
        // used by the batcher. `encoded_length` is needed so that final
        // batched size doesn't exceed the Kinesis limits (5Mb)
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
            finalizers: event.event.take_finalizers(),
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
                partition_key: kinesis_metadata.partition_key.clone(),
            },
            put_records_request: PutRecordsRequestEntry::builder()
                .data(Blob::new(&payload_bytes[..]))
                .partition_key(kinesis_metadata.partition_key)
                .build(),
            finalizers: kinesis_metadata.finalizers,
            metadata,
        }
    }
}
