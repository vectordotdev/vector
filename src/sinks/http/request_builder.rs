//! `RequestBuilder` implementation for the `http` sink.

use std::io;

use bytes::Bytes;

use super::encoder::HttpEncoder;
use crate::sinks::util::encoding::Encoder as _;
use crate::sinks::{http::sink::PartitionKey, prelude::*, util::http::HttpRequest};

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

    fn encode_events(
        &self,
        events: Self::Events,
    ) -> Result<EncodeResult<Self::Payload>, Self::Error> {
        // Same packing as `RequestBuilder::encode_events`. An empty OTLP body stays
        // empty: finishing a compressor with no input still emits a gzip or zstd
        // frame, and posting that retries spans the conversion already consumed.
        let mut compressor = crate::sinks::util::Compressor::from(self.compression());
        let is_compressed = compressor.is_compressed();
        let (written, json_size) = self.encoder().encode_input(events, &mut compressor)?;
        if written == 0 && self.encoder_is_otlp() {
            return Ok(EncodeResult::uncompressed(Bytes::new(), json_size));
        }

        let payload = compressor.into_inner().freeze();
        let result = if is_compressed {
            let compressed_byte_size = payload.len();
            EncodeResult::compressed(payload, compressed_byte_size, json_size)
        } else {
            EncodeResult::uncompressed(payload, json_size)
        };
        Ok(result)
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

impl HttpRequestBuilder {
    fn encoder_is_otlp(&self) -> bool {
        #[cfg(feature = "codecs-opentelemetry")]
        {
            matches!(
                self.encoder.encoder.serializer(),
                vector_lib::codecs::encoding::Serializer::Otlp(_)
            )
        }
        #[cfg(not(feature = "codecs-opentelemetry"))]
        {
            false
        }
    }
}
