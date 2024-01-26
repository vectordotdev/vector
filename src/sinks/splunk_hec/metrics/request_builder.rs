use std::sync::Arc;

use bytes::Bytes;
use vector_lib::event::{EventFinalizers, Finalizable};
use vector_lib::request_metadata::RequestMetadata;

use super::{encoder::HecMetricsEncoder, sink::HecProcessedEvent};
use crate::sinks::{
    splunk_hec::common::request::HecRequest,
    util::{
        metadata::RequestMetadataBuilder, request_builder::EncodeResult, Compression,
        RequestBuilder,
    },
};

pub struct HecMetricsRequestBuilder {
    pub(super) compression: Compression,
}

impl RequestBuilder<(Option<Arc<str>>, Vec<HecProcessedEvent>)> for HecMetricsRequestBuilder {
    type Metadata = (EventFinalizers, Option<Arc<str>>);
    type Events = Vec<HecProcessedEvent>;
    type Encoder = HecMetricsEncoder;
    type Payload = Bytes;
    type Request = HecRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &HecMetricsEncoder
    }

    fn split_input(
        &self,
        input: (Option<Arc<str>>, Vec<HecProcessedEvent>),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (passthrough_token, mut events) = input;
        let finalizers = events.take_finalizers();

        let metadata_builder = RequestMetadataBuilder::from_events(&events);

        ((finalizers, passthrough_token), metadata_builder, events)
    }

    fn build_request(
        &self,
        hec_metadata: Self::Metadata,
        metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let (finalizers, passthrough_token) = hec_metadata;
        HecRequest {
            body: payload.into_payload(),
            finalizers,
            passthrough_token,
            index: None,
            source: None,
            sourcetype: None,
            host: None,
            metadata,
        }
    }
}
