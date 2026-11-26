use bytes::Bytes;
use vector_lib::{
    EstimatedJsonEncodedSizeOf, json_size::JsonSize, request_metadata::RequestMetadata,
};

use crate::{
    event::{EventFinalizers, Finalizable},
    sinks::{
        elasticsearch::{
            encoder::{ElasticsearchEncoder, ProcessedEvent},
            service::ElasticsearchRequest,
        },
        util::{
            Compression, RequestBuilder,
            metadata::RequestMetadataBuilder,
            request_builder::{EncodeResult, RequestParts},
        },
    },
};

#[derive(Debug, Clone)]
pub struct ElasticsearchRequestBuilder {
    pub compression: Compression,
    pub encoder: ElasticsearchEncoder,
}

pub struct Metadata {
    batch_size: usize,
    events_byte_size: JsonSize,
    original_events: Vec<ProcessedEvent>,
}

impl RequestBuilder<Vec<ProcessedEvent>> for ElasticsearchRequestBuilder {
    type Metadata = Metadata;
    type Events = Vec<ProcessedEvent>;
    type Encoder = ElasticsearchEncoder;
    type Payload = Bytes;
    type Request = ElasticsearchRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        mut events: Vec<ProcessedEvent>,
    ) -> RequestParts<Self::Metadata, Self::Events> {
        let events_byte_size = events
            .iter()
            .map(|x| x.log.estimated_json_encoded_size_of())
            .reduce(|a, b| a + b)
            .unwrap_or(JsonSize::zero());

        let metadata_builder = RequestMetadataBuilder::from_events(&events);

        let finalizers = events.take_finalizers();
        let es_metadata = Metadata {
            batch_size: events.len(),
            events_byte_size,
            original_events: events.clone(),
        };
        RequestParts {
            metadata: es_metadata,
            request_metadata_builder: metadata_builder,
            events,
            finalizers,
        }
    }

    fn build_request(
        &self,
        es_metadata: Self::Metadata,
        finalizers: EventFinalizers,
        metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        ElasticsearchRequest {
            payload: payload.into_payload(),
            finalizers,
            batch_size: es_metadata.batch_size,
            events_byte_size: es_metadata.events_byte_size,
            metadata,
            original_events: es_metadata.original_events,
            elasticsearch_request_builder: self.clone(),
        }
    }
}
