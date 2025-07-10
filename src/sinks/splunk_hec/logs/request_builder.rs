use std::sync::Arc;

use bytes::Bytes;
use vector_lib::event::{EventFinalizers, Finalizable};
use vector_lib::request_metadata::RequestMetadata;

use super::{
    encoder::HecLogsEncoder,
    sink::{HecProcessedEvent, Partitioned},
};
use crate::sinks::{
    splunk_hec::common::request::HecRequest,
    util::{
        metadata::RequestMetadataBuilder, request_builder::EncodeResult, Compression,
        RequestBuilder,
    },
};

pub struct HecLogsRequestBuilder {
    pub encoder: HecLogsEncoder,
    pub compression: Compression,
}

#[derive(Debug, Clone)]
pub struct HecRequestMetadata {
    finalizers: EventFinalizers,
    partition: Option<Arc<str>>,
    source: Option<String>,
    sourcetype: Option<String>,
    index: Option<String>,
    host: Option<String>,
}

impl RequestBuilder<(Option<Partitioned>, Vec<HecProcessedEvent>)> for HecLogsRequestBuilder {
    type Metadata = HecRequestMetadata;
    type Events = Vec<HecProcessedEvent>;
    type Encoder = HecLogsEncoder;
    type Payload = Bytes;
    type Request = HecRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: (Option<Partitioned>, Vec<HecProcessedEvent>),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (mut partition, mut events) = input;

        let finalizers = events.take_finalizers();

        let builder = RequestMetadataBuilder::from_events(&events);

        (
            HecRequestMetadata {
                finalizers,
                partition: partition.as_ref().and_then(|p| p.token.clone()),
                source: partition.as_mut().and_then(|p| p.source.take()),
                sourcetype: partition.as_mut().and_then(|p| p.sourcetype.take()),
                index: partition.as_mut().and_then(|p| p.index.take()),
                host: partition.as_mut().and_then(|p| p.host.take()),
            },
            builder,
            events,
        )
    }

    fn build_request(
        &self,
        hec_metadata: Self::Metadata,
        metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        HecRequest {
            body: payload.into_payload(),
            finalizers: hec_metadata.finalizers,
            passthrough_token: hec_metadata.partition,
            source: hec_metadata.source,
            sourcetype: hec_metadata.sourcetype,
            index: hec_metadata.index,
            host: hec_metadata.host,
            metadata,
        }
    }
}
