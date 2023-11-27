use std::{io, marker::PhantomData};

use bytes::Bytes;
use vector_lib::request_metadata::{MetaDescriptive, RequestMetadata};
use vector_lib::ByteSizeOf;

use super::{
    record::Record,
    sink::{KinesisKey, KinesisProcessedEvent},
};
use crate::{
    codecs::{Encoder, Transformer},
    event::{Event, EventFinalizers, Finalizable},
    sinks::util::{
        metadata::RequestMetadataBuilder, request_builder::EncodeResult, Compression,
        RequestBuilder,
    },
};

#[derive(Clone)]
pub struct KinesisRequestBuilder<R> {
    pub compression: Compression,
    pub encoder: (Transformer, Encoder<()>),
    pub _phantom: PhantomData<R>,
}

pub struct KinesisMetadata {
    pub finalizers: EventFinalizers,
    pub partition_key: String,
}

#[derive(Clone)]
pub struct KinesisRequest<R>
where
    R: Record,
{
    pub key: KinesisKey,
    pub record: R,
    pub finalizers: EventFinalizers,
    metadata: RequestMetadata,
}

impl<R> Finalizable for KinesisRequest<R>
where
    R: Record,
{
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl<R> MetaDescriptive for KinesisRequest<R>
where
    R: Record,
{
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

impl<R> ByteSizeOf for KinesisRequest<R>
where
    R: Record,
{
    fn size_of(&self) -> usize {
        // `ByteSizeOf` is being somewhat abused here. This is
        // used by the batcher. `encoded_length` is needed so that final
        // batched size doesn't exceed the Kinesis limits (5Mb)
        self.record.encoded_length()
    }

    fn allocated_bytes(&self) -> usize {
        0
    }
}

impl<R> RequestBuilder<KinesisProcessedEvent> for KinesisRequestBuilder<R>
where
    R: Record,
{
    type Metadata = KinesisMetadata;
    type Events = Event;
    type Encoder = (Transformer, Encoder<()>);
    type Payload = Bytes;
    type Request = KinesisRequest<R>;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        mut processed_event: KinesisProcessedEvent,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let kinesis_metadata = KinesisMetadata {
            finalizers: processed_event.event.take_finalizers(),
            partition_key: processed_event.metadata.partition_key,
        };
        let event = Event::from(processed_event.event);
        let builder = RequestMetadataBuilder::from_event(&event);

        (kinesis_metadata, builder, event)
    }

    fn build_request(
        &self,
        kinesis_metadata: Self::Metadata,
        metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let payload_bytes = payload.into_payload();

        let record = R::new(&payload_bytes, &kinesis_metadata.partition_key);

        KinesisRequest {
            key: KinesisKey {
                partition_key: kinesis_metadata.partition_key.clone(),
            },
            record,
            finalizers: kinesis_metadata.finalizers,
            metadata,
        }
    }
}
