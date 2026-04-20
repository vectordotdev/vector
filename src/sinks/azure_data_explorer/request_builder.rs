//! `RequestBuilder` implementation for the `azure_data_explorer` sink.

use std::io;

use bytes::Bytes;

use super::encoder::AzureDataExplorerEncoder;
use crate::sinks::{prelude::*, util::http::HttpRequest};

pub(super) struct AzureDataExplorerRequestBuilder {
    pub(super) encoder: AzureDataExplorerEncoder,
    pub(super) compression: Compression,
}

impl RequestBuilder<Vec<Event>> for AzureDataExplorerRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = Vec<Event>;
    type Encoder = AzureDataExplorerEncoder;
    type Payload = Bytes;
    type Request = HttpRequest<()>;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        mut events: Vec<Event>,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let finalizers = events.take_finalizers();
        let builder = RequestMetadataBuilder::from_events(&events);
        (finalizers, builder, events)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        HttpRequest::new(payload.into_payload(), metadata, request_metadata, ())
    }
}
