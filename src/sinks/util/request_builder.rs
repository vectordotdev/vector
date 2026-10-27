use std::{io, num::NonZeroUsize};

use bytes::Bytes;
use vector_lib::request_metadata::{GroupedCountByteSize, RequestMetadata};

use super::{Compression, Compressor, encoding::Encoder, metadata::RequestMetadataBuilder};

#[must_use]
pub fn default_request_builder_concurrency_limit() -> NonZeroUsize {
    if let Some(limit) = std::env::var("VECTOR_EXPERIMENTAL_REQUEST_BUILDER_CONCURRENCY")
        .map(|value| value.parse::<NonZeroUsize>().ok())
        .ok()
        .flatten()
    {
        return limit;
    }

    crate::app::worker_threads().unwrap_or(const { NonZeroUsize::new(8).expect("nonzero limit") })
}

pub struct EncodeResult<P> {
    pub payload: P,
    pub uncompressed_byte_size: usize,
    pub transformed_json_size: GroupedCountByteSize,
    pub compressed_byte_size: Option<usize>,
}

impl<P> EncodeResult<P>
where
    P: AsRef<[u8]>,
{
    pub fn uncompressed(payload: P, transformed_json_size: GroupedCountByteSize) -> Self {
        let uncompressed_byte_size = payload.as_ref().len();
        Self {
            payload,
            uncompressed_byte_size,
            transformed_json_size,
            compressed_byte_size: None,
        }
    }

    pub fn compressed(
        payload: P,
        uncompressed_byte_size: usize,
        transformed_json_size: GroupedCountByteSize,
    ) -> Self {
        let compressed_byte_size = payload.as_ref().len();
        Self {
            payload,
            uncompressed_byte_size,
            transformed_json_size,
            compressed_byte_size: Some(compressed_byte_size),
        }
    }
}

impl<P> EncodeResult<P> {
    pub fn into_payload(self) -> P {
        self.payload
    }
}

/// Generalized interface for defining how a batch of events will be turned into a request.
pub trait RequestBuilder<Input> {
    type Metadata;
    type Events;
    type Encoder: Encoder<Self::Events>;
    type Payload: From<Bytes> + AsRef<[u8]>;
    type Request;
    type Error: From<io::Error>;

    /// Gets the compression algorithm used by this request builder.
    fn compression(&self) -> Compression;

    /// Gets the encoder used by this request builder.
    fn encoder(&self) -> &Self::Encoder;

    /// Splits apart the input into the metadata and event portions.
    ///
    /// The metadata should be any information that needs to be passed back to `build_request`
    /// as-is, such as event finalizers, while the events are the actual events to process.
    fn split_input(&self, input: Input) -> (Self::Metadata, RequestMetadataBuilder, Self::Events);

    /// Encodes and compresses the events.
    ///
    /// # Errors
    ///
    /// Returns an error if encoding or writing fails.
    fn encode_events(
        &self,
        events: Self::Events,
    ) -> Result<EncodeResult<Self::Payload>, Self::Error> {
        // TODO: Should we add enough bounds on `Self::Events` that we could automatically derive event count/event byte
        // size, and then we could generate `BatchRequestMetadata` and pass it directly to `build_request`? That would
        // obviate needing to wrap `payload` in `EncodeResult`, although practically speaking.. the name would be kind
        // of clash-y with `Self::Metadata`.
        let mut compressor = Compressor::from(self.compression());
        let is_compressed = compressor.is_compressed();
        let (uncompressed_byte_size, json_size) =
            self.encoder().encode_input(events, &mut compressor)?;

        let payload = compressor.into_inner().freeze();
        let result = if is_compressed {
            EncodeResult::compressed(payload.into(), uncompressed_byte_size, json_size)
        } else {
            EncodeResult::uncompressed(payload.into(), json_size)
        };

        Ok(result)
    }

    /// Builds a request for the given metadata and payload.
    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request;
}

/// Generalized interface for defining how a batch of events will incrementally be turned into requests.
///
/// As opposed to `RequestBuilder`, this trait provides the means to incrementally build requests
/// from a single batch of events, where all events in the batch may not fit into a single request.
/// This can be important for sinks where the underlying service has limitations on the size of a
/// request, or how many events may be present, necessitating a batch be split up into multiple requests.
///
/// While batches can be limited in size before being handed off to a request builder, we can't
/// always know in advance how large the encoded payload will be, which requires us to be able to
/// potentially split a batch into multiple requests.
pub trait IncrementalRequestBuilder<Input> {
    type Metadata;
    type Payload;
    type Request;
    type Error;

    /// Incrementally encodes the given input, potentially generating multiple payloads.
    fn encode_events_incremental(
        &mut self,
        input: Input,
    ) -> Vec<Result<(Self::Metadata, Self::Payload), Self::Error>>;

    /// Builds a request for the given metadata and payload.
    fn build_request(&mut self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::util::encoding::write_all;

    struct BytesEncoder;

    impl Encoder<Vec<u8>> for BytesEncoder {
        fn encode_input(
            &self,
            input: Vec<u8>,
            writer: &mut dyn io::Write,
        ) -> io::Result<(usize, GroupedCountByteSize)> {
            write_all(writer, 1, &input)?;
            Ok((input.len(), GroupedCountByteSize::new_untagged()))
        }
    }

    struct TestRequestBuilder {
        compression: Compression,
    }

    impl RequestBuilder<Vec<u8>> for TestRequestBuilder {
        type Metadata = ();
        type Events = Vec<u8>;
        type Encoder = BytesEncoder;
        type Payload = Bytes;
        type Request = ();
        type Error = io::Error;

        fn compression(&self) -> Compression {
            self.compression
        }

        fn encoder(&self) -> &Self::Encoder {
            &BytesEncoder
        }

        fn split_input(
            &self,
            input: Vec<u8>,
        ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
            ((), RequestMetadataBuilder::default(), input)
        }

        fn build_request(
            &self,
            _metadata: Self::Metadata,
            _request_metadata: RequestMetadata,
            _payload: EncodeResult<Self::Payload>,
        ) -> Self::Request {
        }
    }

    #[test]
    fn encode_events_reports_size_before_compression() {
        let input = b"hello world ".repeat(100);
        let builder = TestRequestBuilder {
            compression: Compression::gzip_default(),
        };

        let result = builder.encode_events(input.clone()).unwrap();
        let compressed_len = result.payload.len();
        assert!(compressed_len < input.len());
        assert_eq!(result.uncompressed_byte_size, input.len());
        assert_eq!(result.compressed_byte_size, Some(compressed_len));

        let metadata = RequestMetadataBuilder::default().build(&result);
        assert_eq!(metadata.request_encoded_size(), input.len());
        assert_eq!(metadata.request_wire_size(), compressed_len);
    }

    #[test]
    fn encode_events_without_compression() {
        let input = b"hello world ".repeat(100);
        let builder = TestRequestBuilder {
            compression: Compression::None,
        };

        let result = builder.encode_events(input.clone()).unwrap();
        assert_eq!(result.payload.len(), input.len());
        assert_eq!(result.uncompressed_byte_size, input.len());
        assert_eq!(result.compressed_byte_size, None);
    }
}
