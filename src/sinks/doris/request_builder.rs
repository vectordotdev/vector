//! `RequestBuilder` implementation for the `Doris` sink.

use super::sink::DorisPartitionKey;
use crate::sinks::{prelude::*, util::http::HttpRequest};
use bytes::Bytes;
use vector_lib::codecs::encoding::Framer;

#[derive(Debug, Clone)]
pub(super) struct DorisRequestBuilder {
    pub(super) compression: Compression,
    pub(super) encoder: (Transformer, Encoder<Framer>),
}

impl RequestBuilder<(DorisPartitionKey, Vec<Event>)> for DorisRequestBuilder {
    type Metadata = DorisPartitionKey;
    type Events = Vec<Event>;
    type Encoder = (Transformer, Encoder<Framer>);
    type Payload = Bytes;
    type Request = HttpRequest<DorisPartitionKey>;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: (DorisPartitionKey, Vec<Event>),
    ) -> RequestParts<Self::Metadata, Self::Events> {
        let (key, mut events) = input;

        let finalizers = events.take_finalizers();
        let builder = RequestMetadataBuilder::from_events(&events);
        RequestParts {
            metadata: key,
            request_metadata_builder: builder,
            events,
            finalizers,
        }
    }

    fn build_request(
        &self,
        key: Self::Metadata,
        finalizers: EventFinalizers,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        HttpRequest::new(
            payload.into_payload(),
            finalizers,
            request_metadata,
            DorisPartitionKey {
                database: key.database,
                table: key.table,
            },
        )
    }
}
