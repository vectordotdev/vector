//! `RequestBuilder` implementation for the `Clickhouse` sink.

use std::sync::Arc;

use bytes::Bytes;
use snafu::Snafu;
use vector_lib::codecs::encoding::Framer;

#[cfg(feature = "sinks-clickhouse")]
use super::encoder;
use super::{config::Format, sink::PartitionKey};
use crate::codecs::Encoder;
use crate::sinks::util::Compressor;
use crate::sinks::{prelude::*, util::http::HttpRequest};

#[derive(Debug, Snafu)]
pub enum RequestBuilderError {
    #[snafu(display("Failed to encode events to Arrow: {}", source))]
    ArrowEncoding { source: encoder::ArrowEncodingError },

    #[snafu(display("Failed to compress payload: {}", source))]
    Compression { source: std::io::Error },

    #[snafu(display("Failed to encode events: {}", source))]
    Encoding { source: std::io::Error },

    #[snafu(display("IO error: {}", source))]
    Io { source: std::io::Error },
}

impl From<std::io::Error> for RequestBuilderError {
    fn from(source: std::io::Error) -> Self {
        RequestBuilderError::Io { source }
    }
}

pub(super) struct ClickhouseRequestBuilder {
    pub(super) compression: Compression,
    pub(super) encoding: (Transformer, Encoder<Framer>),
    pub(super) format: Format,
    #[cfg(feature = "sinks-clickhouse")]
    pub(super) arrow_schema: Option<Arc<arrow::datatypes::Schema>>,
}

impl RequestBuilder<(PartitionKey, Vec<Event>)> for ClickhouseRequestBuilder {
    type Metadata = (PartitionKey, EventFinalizers);
    type Events = Vec<Event>;
    type Encoder = (Transformer, Encoder<Framer>);
    type Payload = Bytes;
    type Request = HttpRequest<PartitionKey>;
    type Error = RequestBuilderError;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoding
    }

    fn split_input(
        &self,
        input: (PartitionKey, Vec<Event>),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (key, mut events) = input;

        let finalizers = events.take_finalizers();
        let builder = RequestMetadataBuilder::from_events(&events);
        ((key, finalizers), builder, events)
    }

    #[cfg(feature = "sinks-clickhouse")]
    fn encode_events(
        &self,
        events: Self::Events,
    ) -> Result<EncodeResult<Self::Payload>, Self::Error> {
        if self.format == Format::ArrowStream {
            return self.build_arrow_request_payload(events);
        }

        // Standard JSON encoding path for other formats
        let mut compressor = Compressor::from(self.compression());
        let is_compressed = compressor.is_compressed();
        let (_, json_size) = {
            use crate::sinks::util::encoding::Encoder as EncoderTrait;
            self.encoder()
                .encode_input(events, &mut compressor)
                .map_err(|source| RequestBuilderError::Encoding { source })?
        };

        let payload = compressor.into_inner().freeze();
        let result = if is_compressed {
            let compressed_byte_size = payload.len();
            EncodeResult::compressed(payload, compressed_byte_size, json_size)
        } else {
            EncodeResult::uncompressed(payload, json_size)
        };

        Ok(result)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let (key, finalizers) = metadata;
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

#[cfg(feature = "sinks-clickhouse")]
impl ClickhouseRequestBuilder {
    fn build_arrow_request_payload(
        &self,
        events: Vec<Event>,
    ) -> Result<EncodeResult<Bytes>, RequestBuilderError> {
        use snafu::ResultExt;

        // Encode events to Arrow IPC format using provided schema
        let arrow_bytes =
            encoder::encode_events_to_arrow_stream(&events, self.arrow_schema.clone())
                .context(ArrowEncodingSnafu)?;

        let uncompressed_byte_size = arrow_bytes.len();

        // Apply compression if enabled
        let mut compressor = Compressor::from(self.compression());
        let is_compressed = compressor.is_compressed();

        use std::io::Write;
        compressor
            .write_all(&arrow_bytes)
            .context(CompressionSnafu)?;

        let payload = compressor.into_inner().freeze();

        // Calculate size for metrics
        // We need to create a temporary EncodeResult to extract the GroupedCountByteSize
        use vector_lib::request_metadata::GroupedCountByteSize;
        let temp_result =
            EncodeResult::uncompressed(Bytes::new(), GroupedCountByteSize::new_untagged());
        let json_size = RequestMetadataBuilder::from_events(&events)
            .build(&temp_result)
            .into_events_estimated_json_encoded_byte_size();

        let result = if is_compressed {
            EncodeResult::compressed(payload, uncompressed_byte_size, json_size)
        } else {
            EncodeResult::uncompressed(payload, json_size)
        };

        Ok(result)
    }
}
