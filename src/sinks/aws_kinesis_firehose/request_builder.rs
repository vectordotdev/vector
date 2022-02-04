use std::io;

use bytes::Bytes;
use rusoto_firehose::Record;
use vector_core::{buffers::Ackable, ByteSizeOf};

use crate::{
    event::{Event, EventFinalizers, Finalizable, LogEvent},
    sinks::util::{
        encoding::{EncodingConfig, StandardEncodings},
        Compression, RequestBuilder,
    },
};

pub struct KinesisRequestBuilder {
    pub compression: Compression,
    pub encoder: EncodingConfig<StandardEncodings>,
}

pub struct Metadata {
    pub finalizers: EventFinalizers,
    pub event_byte_size: usize,
}

#[derive(Clone)]
pub struct KinesisRequest {
    pub record: Record,
    pub finalizers: EventFinalizers,
    pub event_byte_size: usize,
}

impl Ackable for KinesisRequest {
    fn ack_size(&self) -> usize {
        1
    }
}

impl Finalizable for KinesisRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl KinesisRequest {
    fn encoded_length(&self) -> usize {
        // data is simply base64 encoded, quoted, and comma separated
        (self.record.data.len() + 2) / 3 * 4 + 3
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

impl RequestBuilder<LogEvent> for KinesisRequestBuilder {
    type Metadata = Metadata;
    type Events = Event;
    type Encoder = EncodingConfig<StandardEncodings>;
    type Payload = Bytes;
    type Request = KinesisRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(&self, mut event: LogEvent) -> (Self::Metadata, Self::Events) {
        let metadata = Metadata {
            finalizers: event.take_finalizers(),
            event_byte_size: event.size_of(),
        };
        (metadata, Event::from(event))
    }

    fn build_request(&self, metadata: Self::Metadata, data: Bytes) -> Self::Request {
        KinesisRequest {
            record: Record { data },
            finalizers: metadata.finalizers,
            event_byte_size: metadata.event_byte_size,
        }
    }
}
