use vector_core::ByteSizeOf;

use crate::{
    event::{EventFinalizers, Finalizable},
    sinks::{
        elasticsearch::{
            encoder::{ElasticSearchEncoder, ProcessedEvent},
            service::ElasticSearchRequest,
        },
        util::{encoding::EncodingConfigFixed, Compression, RequestBuilder},
    },
};

pub struct ElasticsearchRequestBuilder {
    pub compression: Compression,
    pub encoder: EncodingConfigFixed<ElasticSearchEncoder>,
}

pub struct Metadata {
    finalizers: EventFinalizers,
    batch_size: usize,
    events_byte_size: usize,
}

impl RequestBuilder<Vec<ProcessedEvent>> for ElasticsearchRequestBuilder {
    type Metadata = Metadata;
    type Events = Vec<ProcessedEvent>;
    type Encoder = EncodingConfigFixed<ElasticSearchEncoder>;
    type Payload = Vec<u8>;
    type Request = ElasticSearchRequest;
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

    fn build_request(&self, metadata: Self::Metadata, payload: Vec<u8>) -> Self::Request {
        ElasticSearchRequest {
            payload,
            finalizers: metadata.finalizers,
            batch_size: metadata.batch_size,
            events_byte_size: metadata.events_byte_size,
        }
    }
}
