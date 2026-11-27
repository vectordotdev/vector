use std::sync::Arc;

use bytes::Bytes;
use vector_lib::{
    event::{EventFinalizers, Finalizable},
    request_metadata::RequestMetadata,
};

use super::{encoder::HecMetricsEncoder, sink::HecProcessedEvent};
use crate::sinks::{
    splunk_hec::common::request::HecRequest,
    util::{
        Compression, RequestBuilder,
        metadata::RequestMetadataBuilder,
        request_builder::{EncodeResult, RequestParts},
    },
};

pub(super) struct HecMetricsRequestBuilder {
    pub(super) compression: Compression,
    pub(super) encoder: HecMetricsEncoder,
}

impl HecMetricsRequestBuilder {
    pub(super) const fn new(compression: Compression, templated_field_keys: Box<[String]>) -> Self {
        Self {
            compression,
            encoder: HecMetricsEncoder {
                templated_field_keys,
            },
        }
    }
}

impl RequestBuilder<(Option<Arc<str>>, Vec<HecProcessedEvent>)> for HecMetricsRequestBuilder {
    type Metadata = Option<Arc<str>>;
    type Events = Vec<HecProcessedEvent>;
    type Encoder = HecMetricsEncoder;
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
        input: (Option<Arc<str>>, Vec<HecProcessedEvent>),
    ) -> RequestParts<Self::Metadata, Self::Events> {
        let (passthrough_token, mut events) = input;
        let finalizers = events.take_finalizers();

        let metadata_builder = RequestMetadataBuilder::from_events(&events);

        RequestParts {
            metadata: passthrough_token,
            request_metadata_builder: metadata_builder,
            events,
            finalizers,
        }
    }

    fn build_request(
        &self,
        passthrough_token: Self::Metadata,
        finalizers: EventFinalizers,
        metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
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
