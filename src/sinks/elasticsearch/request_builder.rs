use bytes::Bytes;
use vector_lib::EstimatedJsonEncodedSizeOf;
use vector_lib::{json_size::JsonSize, request_metadata::RequestMetadata};

use crate::{
    event::{EventFinalizers, Finalizable},
    sinks::{
        elasticsearch::{
            encoder::{ElasticsearchEncoder, ProcessedEvent},
            service::ElasticsearchRequest,
        },
        util::{
            metadata::RequestMetadataBuilder, request_builder::EncodeResult, Compression,
            RequestBuilder,
        },
    },
};

#[derive(Debug, Clone)]
pub struct ElasticsearchRequestBuilder {
    pub compression: Compression,
    pub encoder: ElasticsearchEncoder,
}

pub struct Metadata {
    finalizers: EventFinalizers,
    batch_size: usize,
    events_byte_size: JsonSize,
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
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let events_byte_size = events
            .iter()
            .map(|x| x.log.estimated_json_encoded_size_of())
            .reduce(|a, b| a + b)
            .unwrap_or(JsonSize::zero());

        let metadata_builder = RequestMetadataBuilder::from_events(&events);

        let es_metadata = Metadata {
            finalizers: events.take_finalizers(),
            batch_size: events.len(),
            events_byte_size,
        };
        (es_metadata, metadata_builder, events)
    }

    fn build_request(
        &self,
        es_metadata: Self::Metadata,
        metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        ElasticsearchRequest {
            payload: payload.into_payload(),
            finalizers: es_metadata.finalizers,
            batch_size: es_metadata.batch_size,
            events_byte_size: es_metadata.events_byte_size,
            metadata,
        }
    }
}
