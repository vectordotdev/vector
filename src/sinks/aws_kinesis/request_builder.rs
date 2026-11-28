use std::{io, marker::PhantomData};

use bytes::Bytes;
use vector_lib::{
    ByteSizeOf,
    request_metadata::{MetaDescriptive, RequestMetadata},
};

use super::{
    record::Record,
    sink::{KinesisKey, KinesisProcessedEvent},
};
use crate::{
    codecs::{Encoder, Transformer},
    event::{Event, EventFinalizers, Finalizable},
    sinks::util::{
        Compression, RequestBuilder,
        metadata::RequestMetadataBuilder,
        request_builder::{EncodeResult, RequestParts},
    },
};

#[derive(Clone)]
pub struct KinesisRequestBuilder<R> {
    pub compression: Compression,
    pub encoder: (Transformer, Encoder<()>),
    pub _phantom: PhantomData<R>,
}

pub struct KinesisMetadata {
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
    ) -> RequestParts<Self::Metadata, Self::Events> {
        let finalizers = processed_event.event.take_finalizers();
        let kinesis_metadata = KinesisMetadata {
            partition_key: processed_event.metadata.partition_key,
        };
        let event = Event::from(processed_event.event);
        let builder = RequestMetadataBuilder::from_event(&event);

        RequestParts {
            metadata: kinesis_metadata,
            request_metadata_builder: builder,
            events: event,
            finalizers,
        }
    }

    fn build_request(
        &self,
        kinesis_metadata: Self::Metadata,
        finalizers: EventFinalizers,
        metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let payload_bytes = payload.into_payload();

        let record = R::new(&payload_bytes, &kinesis_metadata.partition_key);

        KinesisRequest {
            key: KinesisKey {
                partition_key: kinesis_metadata.partition_key,
            },
            record,
            finalizers,
            metadata,
        }
    }
}
