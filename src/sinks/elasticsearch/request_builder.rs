use bytes::Bytes;
use vector_core::ByteSizeOf;

use crate::{
    event::{EventFinalizers, Finalizable},
    sinks::{
        elasticsearch::{
            encoder::{ElasticsearchEncoder, ProcessedEvent},
            service::ElasticsearchRequest,
        },
        util::{
            encoding::EncodingConfigFixed, request_builder::EncodeResult, Compression,
            RequestBuilder,
        },
    },
};

pub struct ElasticsearchRequestBuilder {
    pub compression: Compression,
    pub encoder: EncodingConfigFixed<ElasticsearchEncoder>,
}

pub struct Metadata {
    finalizers: EventFinalizers,
    batch_size: usize,
    events_byte_size: usize,
}

impl RequestBuilder<Vec<ProcessedEvent>> for ElasticsearchRequestBuilder {
    type Metadata = Metadata;
    type Events = Vec<ProcessedEvent>;
    type Encoder = EncodingConfigFixed<ElasticsearchEncoder>;
    type Payload = Bytes;
    type Request = ElasticsearchRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(&self, mut events: Vec<ProcessedEvent>) -> (Self::Metadata, Self::Events) {
        let events_byte_size = events
            .iter()
            .map(|x| x.log.size_of())
            .reduce(|a, b| a + b)
            .unwrap_or(0);

        let metadata = Metadata {
            finalizers: events.take_finalizers(),
            batch_size: events.len(),
            events_byte_size,
        };
        (metadata, events)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        ElasticsearchRequest {
            payload: payload.into_payload(),
            finalizers: metadata.finalizers,
            batch_size: metadata.batch_size,
            events_byte_size: metadata.events_byte_size,
        }
    }
}
