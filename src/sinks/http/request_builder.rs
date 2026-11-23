//! `RequestBuilder` implementation for the `http` sink.

use std::io;

use bytes::Bytes;

use super::encoder::HttpEncoder;
use crate::sinks::{http::sink::PartitionKey, prelude::*, util::http::HttpRequest};

pub(super) struct HttpRequestBuilder {
    pub(super) encoder: HttpEncoder,
    pub(super) compression: Compression,
}

impl RequestBuilder<(PartitionKey, Vec<Event>)> for HttpRequestBuilder {
    type Metadata = PartitionKey;
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
    ) -> RequestParts<Self::Metadata, Self::Events> {
        let (partition_key, mut events) = input;

        let finalizers = events.take_finalizers();
        let builder = RequestMetadataBuilder::from_events(&events);
        RequestParts {
            metadata: partition_key,
            request_metadata_builder: builder,
            events,
            finalizers,
        }
    }

    fn build_request(
        &self,
        partition_key: Self::Metadata,
        finalizers: EventFinalizers,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        HttpRequest::new(
            payload.into_payload(),
            finalizers,
            request_metadata,
            partition_key,
        )
    }
}
