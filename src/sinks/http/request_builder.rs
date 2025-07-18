//! `RequestBuilder` implementation for the `http` sink.

use bytes::Bytes;
use std::io;

use crate::sinks::{http::sink::PartitionKey, prelude::*, util::http::HttpRequest};

use super::encoder::HttpEncoder;

pub(super) struct HttpRequestBuilder {
    pub(super) encoder: HttpEncoder,
    pub(super) compression: Compression,
}

impl RequestBuilder<(PartitionKey, Vec<Event>)> for HttpRequestBuilder {
    type Metadata = (PartitionKey, EventFinalizers);
    type Events = Vec<Event>;
    type Encoder = HttpEncoder;
    type Payload = Bytes;
    type Request = HttpRequest<PartitionKey>;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: (PartitionKey, Vec<Event>),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (partition_key, mut events) = input;

        let finalizers = events.take_finalizers();
        let builder = RequestMetadataBuilder::from_events(&events);
        ((partition_key, finalizers), builder, events)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let (partition_key, finalizers) = metadata;
        HttpRequest::new(
            payload.into_payload(),
            finalizers,
            request_metadata,
            partition_key,
        )
    }
}
