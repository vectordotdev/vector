//! `RequestBuilder` implementation for the `Clickhouse` sink.

use bytes::Bytes;

use super::sink::PartitionKey;
use crate::sinks::prelude::*;
use crate::sinks::util::http::HttpRequest;
use vector_lib::codecs::EncoderKind;

pub(super) struct ClickhouseRequestBuilder {
    pub(super) compression: Compression,
    pub(super) encoder: (Transformer, EncoderKind),
}

impl RequestBuilder<(PartitionKey, Vec<Event>)> for ClickhouseRequestBuilder {
    type Metadata = PartitionKey;
    type Events = Vec<Event>;
    type Encoder = (Transformer, EncoderKind);
    type Payload = Bytes;
    type Request = HttpRequest<PartitionKey>;
    type Error = std::io::Error;

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
            PartitionKey {
                database: key.database,
                table: key.table,
                format: key.format,
            },
        )
    }
}
