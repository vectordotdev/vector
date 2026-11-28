use std::{io, num::NonZeroUsize};

use bytes::Bytes;
use vector_lib::{
    event::EventFinalizers,
    request_metadata::{GroupedCountByteSize, RequestMetadata},
    sink::PreparationError,
};

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

/// The data and acknowledgement state of one prospective request.
pub struct RequestParts<M, E> {
    pub metadata: M,
    pub request_metadata_builder: RequestMetadataBuilder,
    pub events: E,
    pub finalizers: EventFinalizers,
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
    /// The shared builder holds the finalizers while encoding consumes the events.
    fn split_input(&self, input: Input) -> RequestParts<Self::Metadata, Self::Events>;

    /// Encodes and compresses the events.
    ///
    /// # Errors
    ///
    /// Returns an error if encoding or writing fails.
    fn encode_events(
        &self,
        events: Self::Events,
    ) -> Result<EncodeResult<Self::Payload>, PreparationError<Self::Error>> {
        // TODO: Should we add enough bounds on `Self::Events` that we could automatically derive event count/event byte
        // size, and then we could generate `BatchRequestMetadata` and pass it directly to `build_request`? That would
        // obviate needing to wrap `payload` in `EncodeResult`, although practically speaking.. the name would be kind
        // of clash-y with `Self::Metadata`.
        let mut compressor = Compressor::from(self.compression());
        let is_compressed = compressor.is_compressed();
        let (_, json_size) = encode_to_writer(self.encoder(), events, &mut compressor)?;

        let payload = compressor
            .finish()
            .map_err(|error| PreparationError::Transient(error.into()))?
            .freeze();
        let result = if is_compressed {
            let compressed_byte_size = payload.len();
            EncodeResult::compressed(payload.into(), compressed_byte_size, json_size)
        } else {
            EncodeResult::uncompressed(payload.into(), json_size)
        };

        Ok(result)
    }

    /// Builds a request for the given metadata and payload.
    fn build_request(
        &self,
        metadata: Self::Metadata,
        finalizers: EventFinalizers,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request;
}

fn encode_to_writer<T, E: From<io::Error>>(
    encoder: &impl Encoder<T>,
    events: T,
    writer: &mut dyn io::Write,
) -> Result<(usize, GroupedCountByteSize), PreparationError<E>> {
    encoder
        .encode_input(events, &mut EncodeWriter(writer))
        .map_err(|error| match error.downcast::<OutputWriteError>() {
            Ok(error) => PreparationError::Transient(error.source.into()),
            Err(error) => PreparationError::Permanent(error.into()),
        })
}

/// Tags writer errors so they can be distinguished from serialization errors.
struct EncodeWriter<'a>(&'a mut dyn io::Write);

#[derive(Debug, snafu::Snafu)]
#[snafu(display("{source}"))]
struct OutputWriteError {
    source: io::Error,
}

impl OutputWriteError {
    fn wrap(source: io::Error) -> io::Error {
        io::Error::new(source.kind(), Self { source })
    }
}

impl io::Write for EncodeWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        #[allow(clippy::disallowed_methods)] // Forward short writes to the caller.
        self.0.write(buf).map_err(OutputWriteError::wrap)
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.0.write_all(buf).map_err(OutputWriteError::wrap)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush().map_err(OutputWriteError::wrap)
    }
}

/// One output of incremental encoding, with the finalizers for exactly those events.
pub struct EncodedChunk<M, P, E> {
    pub result: Result<(M, P), PreparationError<E>>,
    pub finalizers: EventFinalizers,
}

pub type IncrementalPayloads<M, P, E> = Vec<EncodedChunk<M, P, E>>;

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
    ) -> IncrementalPayloads<Self::Metadata, Self::Payload, Self::Error>;

    /// Builds a request for the given metadata and payload.
    fn build_request(
        &mut self,
        metadata: Self::Metadata,
        finalizers: EventFinalizers,
        payload: Self::Payload,
    ) -> Self::Request;
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde::Serialize;
    use vector_lib::{
        config::telemetry,
        event::{BatchNotifier, BatchStatus, EventFinalizer},
    };

    use super::*;

    #[tokio::test]
    async fn encoding_distinguishes_serialization_and_writer_failures() {
        struct JsonEncoder;

        impl<T: Serialize> Encoder<T> for JsonEncoder {
            fn encode_input(
                &self,
                input: T,
                writer: &mut dyn io::Write,
            ) -> io::Result<(usize, GroupedCountByteSize)> {
                serde_json::to_writer(writer, &input)?;
                Ok((0, telemetry().create_request_count_byte_size()))
            }
        }

        #[derive(Default)]
        struct FailingWriter {
            bytes_written: usize,
        }

        impl io::Write for FailingWriter {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                if buf.is_empty() {
                    return Ok(0);
                }
                if self.bytes_written != 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "compressor write failed",
                    ));
                }
                self.bytes_written = 1;
                Ok(1)
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let mut writer = FailingWriter::default();
        let write_failure =
            encode_to_writer::<_, io::Error>(&JsonEncoder, 123, &mut writer).unwrap_err();
        assert_eq!(writer.bytes_written, 1);

        let invalid_json = BTreeMap::from([((1, 2), 3)]);
        let serialize_failure =
            encode_to_writer::<_, io::Error>(&JsonEncoder, invalid_json, &mut io::sink())
                .unwrap_err();

        for (failure, expected) in [
            (write_failure, BatchStatus::Errored),
            (serialize_failure, BatchStatus::Rejected),
        ] {
            let (notifier, status) = BatchNotifier::new_with_receiver();
            let finalizers = EventFinalizers::new(EventFinalizer::new(notifier));
            let error = failure.record_failure(&finalizers);
            drop(finalizers);

            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            assert_eq!(status.await, expected);
        }
    }
}
