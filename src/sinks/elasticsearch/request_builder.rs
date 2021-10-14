use crate::sinks::util::{Compression, RequestBuilder};

use crate::sinks::elasticsearch::encoder::{ElasticSearchEncoder, ProcessedEvent};

use crate::sinks::elasticsearch::service::ElasticSearchRequest;

use crate::event::{EventFinalizers, Finalizable};
use crate::sinks::util::encoding::EncodingConfigFixed;

pub struct ElasticsearchRequestBuilder {
    pub compression: Compression,
    pub encoder: EncodingConfigFixed<ElasticSearchEncoder>,
}

pub struct Metadata {
    finalizers: EventFinalizers,
    batch_size: usize,
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
        let metadata = Metadata {
            finalizers: events.take_finalizers(),
            batch_size: events.len(),
        };
        (metadata, events)
    }

    fn build_request(&self, metadata: Self::Metadata, payload: Vec<u8>) -> Self::Request {
        ElasticSearchRequest {
            payload,
            finalizers: metadata.finalizers,
            batch_size: metadata.batch_size,
        }
    }
}
