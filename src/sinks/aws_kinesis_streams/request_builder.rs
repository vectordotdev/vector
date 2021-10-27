use bytes::Bytes;
use rusoto_kinesis::PutRecordsRequestEntry;
use crate::event::{EventFinalizers, Finalizable};
use crate::sinks::aws_kinesis_streams::sink::KinesisProcessedEvent;
use crate::sinks::util::{Compression, RequestBuilder};
use crate::sinks::util::encoding::{EncodingConfig, StandardEncodings};

pub struct KinesisRequestBuilder {
    pub compression: Compression,
    pub encoder: EncodingConfig<StandardEncodings>,
}

// pub struct KinesisRequest {
//
// }

pub struct Metadata {
    pub finalizers: EventFinalizers,
    pub partition_key: String,
}

pub struct KinesisRequest {
    pub put_records_request: PutRecordsRequestEntry,
    pub metadata: Metadata
}

impl RequestBuilder<KinesisProcessedEvent> for KinesisRequestBuilder {
    type Metadata = Metadata;
    type Events = [KinesisProcessedEvent; 1];
    type Encoder = EncodingConfig<StandardEncodings>;
    type Payload = Bytes;
    type Request = KinesisRequest;
    type Error = ();

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(&self, mut event: KinesisProcessedEvent) -> (Self::Metadata, Self::Events) {
        let metadata = Metadata {
            finalizers: event.event.take_finalizers(),
            partition_key: event.partition_key
        };
        let events = [event];
        (metadata, events)
    }

    fn build_request(&self, metadata: Self::Metadata, data: Bytes) -> Self::Request {
        KinesisRequest {
            put_records_request: PutRecordsRequestEntry {
                data,
                partition_key: metadata.partition_key,
                ..Default::default()
            },
            metadata
        }
    }
}
