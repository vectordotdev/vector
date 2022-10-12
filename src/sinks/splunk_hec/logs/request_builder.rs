use std::sync::Arc;

use bytes::Bytes;
use vector_core::event::{EventFinalizers, Finalizable};

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
pub struct RequestMetadata {
    finalizers: EventFinalizers,
    partition: Option<Arc<str>>,
    source: Option<String>,
    sourcetype: Option<String>,
    index: Option<String>,
    host: Option<String>,
    builder: RequestMetadataBuilder,
}

impl RequestBuilder<(Option<Partitioned>, Vec<HecProcessedEvent>)> for HecLogsRequestBuilder {
    type Metadata = RequestMetadata;
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
    ) -> (Self::Metadata, Self::Events) {
        let (mut partition, mut events) = input;

        let finalizers = events.take_finalizers();

        let builder = RequestMetadataBuilder::from_events(&events);

        (
            RequestMetadata {
                finalizers,
                partition: partition.as_ref().and_then(|p| p.token.clone()),
                source: partition.as_mut().and_then(|p| p.source.take()),
                sourcetype: partition.as_mut().and_then(|p| p.sourcetype.take()),
                index: partition.as_mut().and_then(|p| p.index.take()),
                host: partition.as_mut().and_then(|p| p.host.take()),
                builder,
            },
            events,
        )
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        HecRequest {
            body: payload.into_payload(),
            finalizers: metadata.finalizers,
            passthrough_token: metadata.partition,
            source: metadata.source,
            sourcetype: metadata.sourcetype,
            index: metadata.index,
            host: metadata.host,
            metadata: metadata.builder.build(&payload),
        }
    }
}
