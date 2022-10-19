use std::{io, marker::PhantomData};

//use aws_sdk_kinesis::{model::PutRecordsRequestEntry, types::Blob};
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
    //pub put_records_request: PutRecordsRequestEntry,
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
}

pub trait Record {
    type T;

    fn new(payload_bytes: &Bytes, partition_key: &str) -> Self;

    fn encoded_length(&self) -> usize;

    fn get(self) -> Self::T;
}

//impl KinesisRequest {
//    fn encoded_length(&self) -> usize {
//        let hash_key_size = self
//            .put_records_request
//            .explicit_hash_key
//            .as_ref()
//            .map(|s| s.len())
//            .unwrap_or_default();
//
//        // data is base64 encoded
//        let data_len = self
//            .put_records_request
//            .data
//            .as_ref()
//            .map(|data| data.as_ref().len())
//            .unwrap_or(0);
//
//        let key_len = self
//            .put_records_request
//            .partition_key
//            .as_ref()
//            .map(|key| key.len())
//            .unwrap_or(0);
//
//        (data_len + 2) / 3 * 4 + hash_key_size + key_len + 10
//    }
//}

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
    type Metadata = (KinesisMetadata, RequestMetadataBuilder);
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

    fn split_input(&self, mut event: KinesisProcessedEvent) -> (Self::Metadata, Self::Events) {
        let metadata = KinesisMetadata {
            finalizers: event.event.take_finalizers(),
            partition_key: event.metadata.partition_key,
        };
        let event = Event::from(event.event);
        let builder = RequestMetadataBuilder::from_events(&event);

        ((metadata, builder), event)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let (kinesis_metadata, builder) = metadata;
        let metadata = builder.build(&payload);
        let payload_bytes = payload.into_payload();

        //let record = self.record_builder(&payload_bytes[..], kinesis_metadata.partition_key);
        let record = R::new(&payload_bytes, &kinesis_metadata.partition_key);

        KinesisRequest {
            key: KinesisKey {
                partition_key: kinesis_metadata.partition_key.clone(),
            },
            record,
            //put_records_request: PutRecordsRequestEntry::builder()
            //    .data(Blob::new(&payload_bytes[..]))
            //    .partition_key(kinesis_metadata.partition_key)
            //    .build(),
            finalizers: kinesis_metadata.finalizers,
            metadata,
        }
    }
}
