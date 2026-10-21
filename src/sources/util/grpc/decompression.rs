use std::{
    cmp,
    future::Future,
    io::{self, Write},
    mem,
    pin::Pin,
    sync::LazyLock,
    task::{Context, Poll},
};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use flate2::write::GzDecoder;
use futures_util::FutureExt;
use http_1::{HeaderValue, Request, Response};
use http_body_1::Body;
use http_body_util::{
    BodyExt,
    channel::{Channel, Sender},
};
use tokio::{pin, select};
use tonic::{Status, body::BoxBody, metadata::AsciiMetadataValue};
use tower::{Layer, Service};
use vector_lib::internal_event::{
    ByteSize, BytesReceived, InternalEventHandle as _, Protocol, Registered,
};

use crate::internal_events::{GrpcError, GrpcInvalidCompressionSchemeError};
use crate::sources::util::decompression::{
    CappedDecoder, DecompressedSizeLimitExceeded, is_decompressed_size_limit_error,
    max_decompressed_size_bytes, max_zlib_compressed_frame_size_bytes,
};

// Every gRPC message has a five byte header:
// - a compressed flag (u8, 0/1 for compressed/decompressed)
// - a length prefix, indicating the number of remaining bytes to read (u32)
const GRPC_MESSAGE_HEADER_LEN: usize = mem::size_of::<u8>() + mem::size_of::<u32>();
// Fixed container framing a valid frame adds on top of zlib's worst-case expansion. Added to the
// compressed-frame pre-filter so a small cap does not reject a valid zstd frame (or a typical gzip
// frame) whose decompressed size is within the cap.
//
// zstd's frame header is bounded at 22 bytes: 4 magic + 1 frame-header descriptor + 1 window
// descriptor + 4 dictionary ID + 8 frame-content size + 4 content checksum (RFC 8878 §3.1.1), so
// 22 fully covers zstd. gzip's mandatory framing is only 18 bytes (10 header + 8 trailer), but its
// optional FNAME, FCOMMENT and FEXTRA fields are unbounded (RFC 1952 §2.3.1): a gzip frame carrying
// more than this slack in those optional fields could still be rejected here. Encoders don't emit
// them in practice, so 22 covers the realistic case; the prefilter is only a cheap wire-size guard
// and the authoritative per-output cap is still enforced during decompression.
const GRPC_COMPRESSED_FRAME_OVERHEAD_SLACK: usize = 22;
const GRPC_ENCODING_HEADER: &str = "grpc-encoding";
const GRPC_ACCEPT_ENCODING_HEADER: &str = "grpc-accept-encoding";

// The encodings this layer advertises to clients via `grpc-accept-encoding`.
// Each variant maps to a `CompressionScheme` (or `None` for `identity`) through
// `to_scheme`, so adding a variant here forces the decompression match to be
// updated and the advertised list cannot drift from the schemes actually handled.
#[derive(Clone, Copy)]
enum AdvertisedEncoding {
    Gzip,
    Zstd,
    Identity,
}

impl AdvertisedEncoding {
    const ALL: &'static [Self] = &[Self::Gzip, Self::Zstd, Self::Identity];

    const fn as_str(self) -> &'static str {
        match self {
            Self::Gzip => "gzip",
            Self::Zstd => "zstd",
            Self::Identity => "identity",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|e| e.as_str() == s)
    }

    // `identity` is the gRPC no-op encoding: the request body is already
    // uncompressed, so there's nothing to decompress.
    const fn to_scheme(self) -> Option<CompressionScheme> {
        match self {
            Self::Gzip => Some(CompressionScheme::Gzip),
            Self::Zstd => Some(CompressionScheme::Zstd),
            Self::Identity => None,
        }
    }
}

// Advertised to clients via `grpc-accept-encoding`. Derived from
// `AdvertisedEncoding::ALL` so this layer is the single owner of gRPC compression
// negotiation for all Vector gRPC sources and the header value cannot drift from
// the set of schemes actually handled.
static GRPC_ACCEPT_ENCODING_VALUE: LazyLock<String> = LazyLock::new(|| {
    AdvertisedEncoding::ALL
        .iter()
        .map(|e| e.as_str())
        .collect::<Vec<_>>()
        .join(",")
});

enum CompressionScheme {
    Gzip,
    Zstd,
}

impl CompressionScheme {
    fn from_encoding_header(req: &Request<BoxBody>) -> Result<Option<Self>, Status> {
        req.headers()
            .get(GRPC_ENCODING_HEADER)
            .map(|s| {
                s.to_str().map(ToString::to_string).map_err(|_| {
                    Status::unimplemented(format!(
                        "`{GRPC_ENCODING_HEADER}` contains non-visible characters and is not a valid encoding"
                    ))
                })
            })
            .transpose()
            .and_then(|value| match value {
                None => Ok(None),
                Some(scheme) => match AdvertisedEncoding::parse(&scheme) {
                    Some(encoding) => Ok(encoding.to_scheme()),
                    None => Err(Status::unimplemented(format!(
                        "compression scheme `{scheme}` is not supported"
                    ))),
                },
            })
            .map_err(|mut status| {
                status.metadata_mut().insert(
                    GRPC_ACCEPT_ENCODING_HEADER,
                    AsciiMetadataValue::try_from(GRPC_ACCEPT_ENCODING_VALUE.as_str())
                        .expect("advertised encoding value must be valid ASCII"),
                );
                status
            })
    }
}

#[derive(Default)]
enum State {
    #[default]
    WaitingForHeader,
    Forward {
        overall_len: usize,
    },
    Decompress {
        remaining: usize,
    },
}

/// The stage of the decompression pipeline an `io::Error` originated from, used to give the
/// returned `internal` status a stage-specific prefix so failures stay diagnosable.
#[derive(strum::Display)]
#[strum(serialize_all = "lowercase")]
#[derive(Clone, Copy)]
enum DecompressStage {
    Initialize,
    Write,
    Finalize,
}

/// Maps a decompressor `io::Error` to a gRPC [`Status`]: an oversized payload becomes
/// `out_of_range` (a client fault, matching the existing >4GB handling) while anything else falls
/// back to `internal` with the stage the error came from and the underlying error appended, so
/// failures such as a corrupt or truncated payload stay diagnosable from the returned status.
fn decompressor_error_to_status(error: &io::Error, stage: DecompressStage) -> Status {
    if is_decompressed_size_limit_error(error) {
        Status::out_of_range("decompressed message exceeds the maximum allowed size")
    } else {
        Status::internal(format!("failed to {stage} decompressor: {error}"))
    }
}

/// A `Write` sink that appends into a `Vec` but refuses to grow past `max_len`, so a streaming
/// decompressor errors out *during* decompression rather than first materializing an oversized
/// output and only then having its size checked.
struct LimitedWriter {
    buf: Vec<u8>,
    max_len: usize,
}

impl LimitedWriter {
    const fn new(buf: Vec<u8>, max_len: usize) -> Self {
        Self { buf, max_len }
    }

    fn into_inner(self) -> Vec<u8> {
        self.buf
    }
}

impl Write for LimitedWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if self.buf.len().saturating_add(data.len()) > self.max_len {
            return Err(io::Error::other(DecompressedSizeLimitExceeded));
        }
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

enum Decompressor {
    Gzip {
        decoder: Box<GzDecoder<LimitedWriter>>,
    },
    Zstd {
        compressed: Vec<u8>,
        output_buf: Vec<u8>,
        limit: usize,
    },
}

impl Decompressor {
    #[allow(
        clippy::unnecessary_wraps,
        reason = "Removing this fallible API is deferred to keep source construction and error handling unchanged. https://github.com/vectordotdev/vector/issues/23659"
    )]
    fn new(scheme: &CompressionScheme) -> Result<Self, io::Error> {
        // Create the backing buffer for the decompressor and set the compression flag to false (0)
        // and pre-allocate the space for the length prefix, which we'll fill out once we've
        // finalized the decompressor.
        let buf = vec![0; GRPC_MESSAGE_HEADER_LEN];
        // Cap the decompressed output so a compression bomb on this unauthenticated gRPC
        // listener cannot drive unbounded allocation.
        let limit = max_decompressed_size_bytes();
        match scheme {
            // The gzip output buffer already holds the 5-byte header, so the sink may grow to the
            // header plus the decompressed cap; anything larger errors mid-decompression.
            CompressionScheme::Gzip => Ok(Decompressor::Gzip {
                decoder: Box::new(GzDecoder::new(LimitedWriter::new(
                    buf,
                    GRPC_MESSAGE_HEADER_LEN.saturating_add(limit),
                ))),
            }),
            CompressionScheme::Zstd => Ok(Decompressor::Zstd {
                compressed: Vec::new(),
                output_buf: buf,
                limit,
            }),
        }
    }

    fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        match self {
            // The `LimitedWriter` bounds the decompressed output, so a gzip bomb errors here rather
            // than materializing an oversized buffer before the size is checked.
            Decompressor::Gzip { decoder } => decoder.write_all(data),
            Decompressor::Zstd { compressed, .. } => {
                compressed.extend_from_slice(data);
                Ok(())
            }
        }
    }

    fn finish(self) -> io::Result<Vec<u8>> {
        match self {
            Decompressor::Gzip { decoder } => (*decoder).finish().map(LimitedWriter::into_inner),
            // Decode directly into output_buf to avoid a temporary intermediate Vec that
            // decode_all would produce; peak memory is compressed + decompressed rather than
            // compressed + 2 × decompressed. The output is bounded by `limit` via
            // CappedDecoder, and zstd's internal window is bounded to match so a crafted
            // frame cannot force a large window allocation.
            Decompressor::Zstd {
                compressed,
                mut output_buf,
                limit,
            } => {
                // The `CappedReader` errors out once the decompressed output would exceed the cap,
                // so a zstd bomb surfaces as a size-limit error here rather than overflowing
                // `output_buf`.
                let mut reader =
                    CappedDecoder::zstd_with_limit(io::Cursor::new(&compressed), limit)?
                        .into_reader();
                io::copy(&mut reader, &mut output_buf)?;
                Ok(output_buf)
            }
        }
    }
}

/// Splits request body chunks into gRPC messages and decompresses the compressed ones.
///
/// Emitted messages are uncompressed, with a length prefix that matches their payload.
struct MessageDecoder {
    scheme: Option<CompressionScheme>,
    state: State,
    buf: BytesMut,
    decompressor: Option<Decompressor>,
    bytes_received: usize,
}

impl MessageDecoder {
    fn new(scheme: Option<CompressionScheme>) -> Self {
        Self {
            scheme,
            state: State::default(),
            buf: BytesMut::new(),
            decompressor: None,
            bytes_received: 0,
        }
    }

    /// Total size of all emitted messages, including their headers, after decompression.
    const fn bytes_received(&self) -> usize {
        self.bytes_received
    }

    /// Buffers `chunk` and returns the next complete message, if any. Later complete messages
    /// stay buffered until the next chunk.
    fn process_chunk(&mut self, chunk: Bytes) -> Result<Option<Bytes>, Status> {
        self.buf.put(chunk);

        loop {
            match self.state {
                State::WaitingForHeader => {
                    // If we don't have enough data yet to even read the gRPC message header, we can't do anything yet.
                    if self.buf.len() < GRPC_MESSAGE_HEADER_LEN {
                        return Ok(None);
                    }

                    self.start_message()?;
                }
                State::Forward { overall_len } => {
                    // All we're doing at this point is waiting until we have all the bytes for the current gRPC message
                    // before we emit them to the caller.
                    if self.buf.len() < overall_len {
                        return Ok(None);
                    }

                    // Now that we have all the bytes we need, slice them out of our internal buffer, reset our state,
                    // and hand the message back to the caller.
                    let message = self.buf.split_to(overall_len).freeze();
                    self.state = State::WaitingForHeader;

                    self.bytes_received += overall_len;

                    return Ok(Some(message));
                }
                // We don't need any more data, so finalize the decompressed message.
                State::Decompress { remaining: 0 } => return self.finish_message().map(Some),
                State::Decompress { remaining } => {
                    // We're waiting for `remaining` more bytes to feed to the decompressor before we finalize it and
                    // generate our new chunk of data. We might have data in our internal buffer, so try and drain that
                    // first before waiting for the next chunk.
                    if self.buf.is_empty() {
                        return Ok(None);
                    }

                    self.feed_decompressor(remaining)?;
                }
            }
        }
    }

    /// Parses the buffered message header and selects how the message payload is processed.
    fn start_message(&mut self) -> Result<(), Status> {
        // Extract the compressed flag and length prefix.
        let (is_compressed, message_len) = {
            let header = &self.buf[..GRPC_MESSAGE_HEADER_LEN];

            let message_len_raw: u32 = header[1..]
                .try_into()
                .map(u32::from_be_bytes)
                .expect("there must be four bytes remaining in the header slice");
            let message_len = message_len_raw
                .try_into()
                .expect("Vector does not support 16-bit platforms");

            (header[0] == 1, message_len)
        };

        // Now, if the message is not compressed, then put ourselves into forward mode, where we'll wait for
        // the rest of the message to come in -- decoding isn't streaming so there's no benefit there --
        // before we emit it.
        //
        // If the message _is_ compressed, we do roughly the same thing but we shove it into the
        // decompressor incrementally because there's no good reason to make both the internal buffer and
        // the decompressor buffer expand if we don't have to.
        if is_compressed {
            // Per the gRPC compression spec, the compressed flag requires a
            // negotiated encoding. Reject frames that set it under identity
            // (or with no `grpc-encoding` header) rather than silently
            // falling back to gzip and masking client/server mismatches.
            if self.scheme.is_none() {
                return Err(Status::internal(
                    "received compressed frame but no compression scheme was negotiated",
                ));
            }

            // Reject a compressed payload whose declared wire size could not
            // legitimately decompress within the cap, before we buffer any of it. The
            // bound (decompressed cap plus zlib's worst-case expansion, shared with the
            // logstash source) keeps a peer from advertising a huge length and
            // slow-streaming bytes to grow the decompressor's input buffer unbounded.
            //
            // The zlib expansion factor covers gzip and zstd too, but those formats add
            // a few bytes of fixed container framing (gzip header/trailer, zstd frame
            // header) on top. Add a small fixed slack so a tiny cap cannot falsely
            // reject a valid gzip/zstd frame whose decompressed size is within the cap;
            // the authoritative per-output cap is still enforced during decompression.
            let compressed_frame_limit = max_zlib_compressed_frame_size_bytes()
                .saturating_add(GRPC_COMPRESSED_FRAME_OVERHEAD_SLACK);
            if message_len > compressed_frame_limit {
                return Err(Status::out_of_range(
                    "compressed message length exceeds the maximum allowed size",
                ));
            }

            // We skip the header in the buffer because it doesn't matter to the decompressor and we
            // recreate it anyways.
            self.buf.advance(GRPC_MESSAGE_HEADER_LEN);

            self.state = State::Decompress {
                remaining: message_len,
            };
        } else {
            // Reject an identity (uncompressed) message larger than the cap before
            // buffering it to `overall_len`, so a large declared length cannot drive
            // unbounded buffering here ahead of tonic's own decode-size limit.
            if message_len > max_decompressed_size_bytes() {
                return Err(Status::out_of_range(
                    "message length exceeds the maximum allowed size",
                ));
            }

            let overall_len = GRPC_MESSAGE_HEADER_LEN + message_len;
            self.state = State::Forward { overall_len };
        }

        Ok(())
    }

    /// Feeds buffered payload bytes of the current compressed message to the decompressor.
    fn feed_decompressor(&mut self, remaining: usize) -> Result<(), Status> {
        // Write the lesser of what the buffer has, or what is remaining for the current message, into
        // the decompressor. This is _technically_ synchronous but there's really no way to do it
        // asynchronously since we already have the data, and that's the only asynchronous part.
        let to_take = cmp::min(self.buf.len(), remaining);
        let decompressor = match &mut self.decompressor {
            Some(decompressor) => decompressor,
            slot @ None => {
                let scheme = self
                    .scheme
                    .as_ref()
                    .expect("compressed frames without a negotiated scheme are rejected earlier");
                slot.insert(Decompressor::new(scheme).map_err(|error| {
                    decompressor_error_to_status(&error, DecompressStage::Initialize)
                })?)
            }
        };
        if let Err(error) = decompressor.write_all(&self.buf[..to_take]) {
            return Err(decompressor_error_to_status(&error, DecompressStage::Write));
        }

        self.state = State::Decompress {
            remaining: remaining - to_take,
        };
        self.buf.advance(to_take);

        Ok(())
    }

    /// Finalizes the decompressor for the current message and returns the decompressed message.
    fn finish_message(&mut self) -> Result<Bytes, Status> {
        // Consume the decompressor, finalize it by updating the length prefix, and then pass it back to the caller.
        let result = self
            .decompressor
            .take()
            .expect("consumed decompressor when no decompressor was present")
            .finish();

        // Decompression can fail here because the payload exceeded the size cap
        // (an oversized-request client fault, mapped to `out_of_range`) or because
        // finalization is where the decompressor validates the integrity of the
        // payload (for gzip, the deflate stream must be complete and the
        // CRC32/length trailer must match; for zstd, the frame must decode
        // cleanly). Anything besides the size cap means the client sent a corrupt
        // or truncated message, surfaced as an internal error with the underlying
        // cause attached so the failure is diagnosable.
        let mut buf = result
            .map_err(|error| decompressor_error_to_status(&error, DecompressStage::Finalize))?;
        self.bytes_received += buf.len();

        // Write the length of our decompressed message in the pre-allocated slot for the message's length prefix.
        let message_len_actual = buf.len() - GRPC_MESSAGE_HEADER_LEN;
        let message_len = u32::try_from(message_len_actual)
            .map_err(|_| Status::out_of_range("messages greater than 4GB are not supported"))?;

        let message_len_bytes = message_len.to_be_bytes();
        let message_len_slot = &mut buf[1..GRPC_MESSAGE_HEADER_LEN];
        message_len_slot.copy_from_slice(&message_len_bytes[..]);

        // Reset our state before returning the decompressed message.
        self.state = State::WaitingForHeader;

        Ok(buf.into())
    }
}

/// Creates the channel that carries decompressed messages to the inner service.
///
/// The channel buffers at most one unread frame, so forwarding the next message waits for the
/// inner service to consume the previous one, and the upstream body is only read on demand.
fn decompressed_body_channel() -> (Sender<Bytes, Status>, Channel<Bytes, Status>) {
    Channel::new(1)
}

async fn drive_body_decompression<B>(
    mut source: B,
    mut destination: Sender<Bytes, Status>,
    scheme: Option<CompressionScheme>,
) -> Result<usize, Status>
where
    B: Body<Data = Bytes> + Unpin,
{
    let mut decoder = MessageDecoder::new(scheme);

    while let Some(result) = source.frame().await {
        let frame = result.map_err(|_| Status::internal("failed to read from underlying body"))?;

        match frame.into_data() {
            Ok(chunk) => {
                if let Some(message) = decoder.process_chunk(chunk)? {
                    // We got a decompressed (or passthrough) message chunk, so just forward it to the destination.
                    if destination.send_data(message).await.is_err() {
                        return Err(Status::internal("destination body abnormally closed"));
                    }
                }
            }
            // Trailers end the underlying body, so forward them after all of its messages.
            Err(frame) => {
                if let Ok(trailers) = frame.into_trailers()
                    && destination.send_trailers(trailers).await.is_err()
                {
                    return Err(Status::internal("destination body abnormally closed"));
                }
            }
        }
    }

    Ok(decoder.bytes_received())
}

async fn drive_request<B, F, E>(
    source: B,
    destination: Sender<Bytes, Status>,
    inner: F,
    bytes_received: Registered<BytesReceived>,
    scheme: Option<CompressionScheme>,
) -> Result<Response<BoxBody>, E>
where
    B: Body<Data = Bytes> + Unpin,
    F: Future<Output = Result<Response<BoxBody>, E>>,
    E: std::fmt::Display,
{
    let body_decompression = drive_body_decompression(source, destination, scheme);

    pin!(inner);
    pin!(body_decompression);

    let mut body_eof = false;
    let mut body_bytes_received = 0;

    let mut result = loop {
        select! {
            biased;

            // Drive the inner future, as this will be consuming the message chunks we give it.
            result = &mut inner => break result,

            // Drive the core decompression loop, reading chunks from the underlying body, decompressing them if needed,
            // and eventually handling trailers at the end, if they're present.
            result = &mut body_decompression, if !body_eof => match result {
                Err(e) => break Ok(e.into_http()),
                Ok(bytes_received) => {
                    body_bytes_received = bytes_received;
                    body_eof = true;
                },
            }
        }
    };

    // If the response indicates success, then emit the necessary metrics
    // otherwise emit the error.
    match &result {
        Ok(res) if res.status().is_success() => {
            bytes_received.emit(ByteSize(body_bytes_received));
        }
        Ok(res) => {
            emit!(GrpcError {
                error: format!("Received {}", res.status())
            });
        }
        Err(error) => {
            emit!(GrpcError { error: &error });
        }
    }

    // Advertise the set of compression schemes this layer can accept to the client.
    // Since this layer is the single owner of compression negotiation, individual
    // services no longer call `.accept_compressed(..)` and therefore tonic would not
    // set this header itself.
    if let Ok(res) = result.as_mut() {
        res.headers_mut().insert(
            GRPC_ACCEPT_ENCODING_HEADER,
            HeaderValue::from_str(&GRPC_ACCEPT_ENCODING_VALUE)
                .expect("advertised encoding value must be valid ASCII"),
        );
    }

    result
}

#[derive(Clone)]
pub struct DecompressionAndMetrics<S> {
    inner: S,
    bytes_received: Registered<BytesReceived>,
}

impl<S> Service<Request<BoxBody>> for DecompressionAndMetrics<S>
where
    S: Service<Request<BoxBody>, Response = Response<BoxBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: std::fmt::Display,
{
    type Response = Response<BoxBody>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<BoxBody>) -> Self::Future {
        match CompressionScheme::from_encoding_header(&req) {
            // There was a header for the encoding, but it was either invalid data or a scheme we don't support.
            Err(status) => {
                emit!(GrpcInvalidCompressionSchemeError { status: &status });
                Box::pin(async move { Ok(status.into_http()) })
            }

            // The request either isn't using compression, or it has indicated compression may be used and we know we
            // can support decompression based on the indicated compression scheme... so wrap the body to decompress, if
            // need be, and then track the bytes that flowed through.
            Ok(scheme) => {
                let (destination, decompressed_body) = decompressed_body_channel();
                let (mut req_parts, req_body) = req.into_parts();
                // Since this layer owns compression negotiation and is about to hand the
                // inner service a fully decompressed body (with the per-message compressed
                // flag cleared), strip the `grpc-encoding` header so tonic's codegen treats
                // the request as uncompressed and does not try to validate the encoding
                // against any per-service `accept_compressed(..)` configuration.
                if scheme.is_some() {
                    req_parts.headers.remove(GRPC_ENCODING_HEADER);
                }
                let mapped_req = Request::from_parts(req_parts, decompressed_body.boxed_unsync());

                let inner = self.inner.call(mapped_req);

                drive_request(
                    req_body,
                    destination,
                    inner,
                    self.bytes_received.clone(),
                    scheme,
                )
                .boxed()
            }
        }
    }
}

/// A layer for decompressing Tonic request payloads and emitting telemetry for the payload sizes.
///
/// In some cases, we configure `tonic` to use compression on requests to save CPU and throughput when sending those
/// large requests. In the case of Vector-to-Vector communication, this means the Vector v2 source may deal with
/// compressed requests. The code already transparently handles decompression, but as part of our component
/// specification, we have specific goals around what event representations we pay attention to.
///
/// In the case of tracking bytes sent/received, we always want to track the number of bytes received _after_
/// decompression to faithfully represent the amount of data being processed by Vector. This poses a problem with the
/// out-of-the-box `tonic` codegen as there is no hook whatsoever to inspect the raw request payload (after
/// decompression, if it was compressed at all) prior to the payload being decoded as a Protocol Buffers payload.
///
/// This layer wraps the incoming body in our own body type, which allows us to do two things: decompress the payload
/// before it enters the decoding phase, and emit metrics based on the decompressed payload.
///
/// Since we can see the decompressed bytes, and also know if the underlying service responded successfully -- i.e. the
/// request was valid, and was processed -- we can now report the number of bytes (after decompression) that were
/// received _and_ processed correctly.
///
/// The supported compression schemes are gzip and zstd.
#[derive(Clone, Default)]
pub struct DecompressionAndMetricsLayer;

impl<S> Layer<S> for DecompressionAndMetricsLayer {
    type Service = DecompressionAndMetrics<S>;

    fn layer(&self, inner: S) -> Self::Service {
        DecompressionAndMetrics {
            inner,
            bytes_received: register!(BytesReceived::from(Protocol::from("grpc"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        convert::Infallible,
        io::Read as _,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use flate2::{Compression, read::GzEncoder};
    use futures::{Stream, StreamExt as _, stream};
    use http_1::HeaderMap;
    use http_body_1::Frame;
    use http_body_util::{Full, StreamBody};
    use tokio_test::{assert_pending, assert_ready, task};
    use tonic::Code;

    use super::*;

    fn grpc_header(compressed: bool, len: usize) -> Vec<u8> {
        let mut header = Vec::with_capacity(GRPC_MESSAGE_HEADER_LEN);
        header.push(u8::from(compressed));
        header.extend_from_slice(
            &u32::try_from(len)
                .expect("test lengths must fit in a gRPC length prefix")
                .to_be_bytes(),
        );
        header
    }

    fn grpc_frame(compressed: bool, payload: &[u8]) -> Vec<u8> {
        let mut frame = grpc_header(compressed, payload.len());
        frame.extend_from_slice(payload);
        frame
    }

    // Mirrors the sender path: `tonic` compresses each message with
    // `flate2::read::GzEncoder` at compression level 6.
    fn gzip_compress(payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        GzEncoder::new(payload, Compression::new(6))
            .read_to_end(&mut out)
            .expect("gzip compression cannot fail");
        out
    }

    fn compress(scheme: &CompressionScheme, payload: &[u8]) -> Vec<u8> {
        match scheme {
            CompressionScheme::Gzip => gzip_compress(payload),
            CompressionScheme::Zstd => {
                zstd::encode_all(payload, 0).expect("zstd compression cannot fail")
            }
        }
    }

    /// Feeds `wire` to `decoder` in `chunk_len`-byte chunks and collects every emitted message.
    fn decode_in_chunks(
        decoder: &mut MessageDecoder,
        wire: &[u8],
        chunk_len: usize,
    ) -> Result<Vec<Bytes>, Status> {
        let mut messages = Vec::new();
        for chunk in wire.chunks(chunk_len) {
            messages.extend(decoder.process_chunk(Bytes::copy_from_slice(chunk))?);
        }
        Ok(messages)
    }

    fn decode_error(scheme: Option<CompressionScheme>, chunk: Vec<u8>) -> Code {
        MessageDecoder::new(scheme)
            .process_chunk(Bytes::from(chunk))
            .expect_err("the message must be rejected")
            .code()
    }

    #[derive(Debug)]
    struct Decompressed {
        data: Vec<u8>,
        bytes_received: usize,
        trailers: Option<HeaderMap>,
    }

    async fn decompress_body<B>(
        scheme: Option<CompressionScheme>,
        body: B,
    ) -> Result<Decompressed, Status>
    where
        B: Body<Data = Bytes> + Unpin,
    {
        let (destination, mut decompressed) = decompressed_body_channel();

        let read_all = async {
            let mut data = Vec::new();
            let mut trailers = None;
            while let Some(frame) = decompressed.frame().await {
                match frame
                    .expect("the destination body must not fail")
                    .into_data()
                {
                    Ok(chunk) => data.extend_from_slice(&chunk),
                    Err(frame) => trailers = frame.into_trailers().ok(),
                }
            }
            (data, trailers)
        };

        let (result, (data, trailers)) = tokio::join!(
            drive_body_decompression(body, destination, scheme),
            read_all
        );

        result.map(|bytes_received| Decompressed {
            data,
            bytes_received,
            trailers,
        })
    }

    /// Returns an upstream body holding `data` in a single DATA frame.
    fn full_body(data: Vec<u8>) -> Full<Bytes> {
        Full::new(Bytes::from(data))
    }

    /// Returns an upstream body yielding every chunk of `chunks` as a DATA frame.
    fn stream_body<S, E>(chunks: S) -> StreamBody<impl Stream<Item = Result<Frame<Bytes>, E>>>
    where
        S: Stream<Item = Result<Vec<u8>, E>>,
    {
        StreamBody::new(chunks.map(|chunk| chunk.map(|chunk| Frame::data(Bytes::from(chunk)))))
    }

    /// Yields `chunks` as an upstream body stream, counting every chunk read from it.
    fn counting_stream(
        chunks: Vec<Vec<u8>>,
        pulled: &Arc<AtomicUsize>,
    ) -> impl Stream<Item = Result<Vec<u8>, Infallible>> + Send + 'static {
        let pulled = Arc::clone(pulled);
        stream::iter(chunks).map(move |chunk| {
            pulled.fetch_add(1, Ordering::SeqCst);
            Ok(chunk)
        })
    }

    /// Returns the next message already buffered in `body`.
    fn next_message(body: &mut Channel<Bytes, Status>) -> Bytes {
        body.frame()
            .now_or_never()
            .expect("a message must be buffered")
            .expect("the body must not have ended")
            .expect("the body must not have failed")
            .into_data()
            .expect("the frame must carry a message")
    }

    fn identity_frames(count: usize) -> Vec<Vec<u8>> {
        (0..count)
            .map(|index| grpc_frame(false, format!("message {index}").as_bytes()))
            .collect()
    }

    fn accept_encoding(response: &Response<BoxBody>) -> Option<&str> {
        response
            .headers()
            .get(GRPC_ACCEPT_ENCODING_HEADER)
            .and_then(|value| value.to_str().ok())
    }

    #[test]
    fn decoder_reassembles_split_mixed_messages() {
        let small = b"a small gRPC message payload".repeat(10);
        let large = b"a large gRPC message payload".repeat(100);
        let expected = [
            grpc_frame(false, &small),
            grpc_frame(false, &large),
            grpc_frame(false, b""),
            grpc_frame(false, &small),
        ];
        let expected_bytes = expected.iter().map(Vec::len).sum::<usize>();

        // Every frame is at least five bytes long, so no chunk of up to five bytes completes two
        // messages, and these chunkings split headers and payloads at every offset.
        for chunk_len in 1..=GRPC_MESSAGE_HEADER_LEN {
            for scheme in [CompressionScheme::Gzip, CompressionScheme::Zstd] {
                let wire = [
                    grpc_frame(false, &small),
                    grpc_frame(true, &compress(&scheme, &large)),
                    grpc_frame(false, b""),
                    grpc_frame(true, &compress(&scheme, &small)),
                ]
                .concat();
                let mut decoder = MessageDecoder::new(Some(scheme));

                let messages = decode_in_chunks(&mut decoder, &wire, chunk_len)
                    .expect("valid messages must decode");

                assert_eq!(messages, expected);
                // Bytes are counted after decompression, including each message header.
                assert_eq!(decoder.bytes_received(), expected_bytes);
            }
        }
    }

    #[test]
    fn decoder_checks_declared_lengths_before_buffering_payloads() {
        let identity_limit = max_decompressed_size_bytes();
        let compressed_limit =
            max_zlib_compressed_frame_size_bytes() + GRPC_COMPRESSED_FRAME_OVERHEAD_SLACK;

        // Lengths at the limits wait for their payloads.
        let message = MessageDecoder::new(None)
            .process_chunk(Bytes::from(grpc_header(false, identity_limit)))
            .expect("an identity length at the limit must be accepted");
        assert_eq!(message, None);
        let message = MessageDecoder::new(Some(CompressionScheme::Gzip))
            .process_chunk(Bytes::from(grpc_header(true, compressed_limit)))
            .expect("a compressed length at the limit must be accepted");
        assert_eq!(message, None);

        assert_eq!(
            decode_error(None, grpc_header(false, identity_limit + 1)),
            Code::OutOfRange
        );
        assert_eq!(
            decode_error(
                Some(CompressionScheme::Zstd),
                grpc_header(true, compressed_limit + 1)
            ),
            Code::OutOfRange
        );
    }

    #[tokio::test]
    async fn forwards_uncompressed_frame() {
        let payload = b"an uncompressed gRPC message payload".repeat(100);
        let frame = grpc_frame(false, &payload);

        let out = decompress_body(None, full_body(frame.clone()))
            .await
            .expect("passthrough must succeed");

        assert_eq!(out.data, frame);
    }

    #[tokio::test]
    async fn decompresses_gzip_frame() {
        let payload = b"a gzip-compressed gRPC message payload".repeat(100);
        let frame = grpc_frame(true, &gzip_compress(&payload));

        let out = decompress_body(Some(CompressionScheme::Gzip), full_body(frame))
            .await
            .expect("decompression must succeed");

        assert_eq!(out.data, grpc_frame(false, &payload));
    }

    #[tokio::test]
    async fn decompresses_gzip_frame_across_chunk_boundaries() {
        let payload = b"a gzip-compressed gRPC message payload".repeat(100);
        let frame = grpc_frame(true, &gzip_compress(&payload));

        // Deliver the frame in small chunks to exercise the incremental paths of the
        // state machine, including chunk boundaries inside the gRPC message header
        // and inside the gzip header/trailer.
        let chunks = frame
            .chunks(3)
            .map(|chunk| Ok::<_, Infallible>(chunk.to_vec()))
            .collect::<Vec<_>>();
        let body = stream_body(stream::iter(chunks));

        let out = decompress_body(Some(CompressionScheme::Gzip), body)
            .await
            .expect("decompression must succeed");

        assert_eq!(out.data, grpc_frame(false, &payload));
    }

    #[tokio::test]
    async fn decompresses_zstd_frame() {
        let payload = b"a zstd-compressed gRPC message payload".repeat(100);
        let frame = grpc_frame(true, &compress(&CompressionScheme::Zstd, &payload));

        let out = decompress_body(Some(CompressionScheme::Zstd), full_body(frame))
            .await
            .expect("decompression must succeed");

        assert_eq!(out.data, grpc_frame(false, &payload));
    }

    #[tokio::test]
    async fn rejects_corrupt_gzip_trailer() {
        let payload = b"payload whose gzip CRC32 trailer gets corrupted".repeat(100);
        let mut compressed = gzip_compress(&payload);
        // The last eight bytes of a gzip stream are the CRC32 and length trailer;
        // corruption there is only detectable when the decompressor is finalized.
        let crc_offset = compressed.len() - 8;
        compressed[crc_offset] ^= 0xff;

        let status = decompress_body(
            Some(CompressionScheme::Gzip),
            full_body(grpc_frame(true, &compressed)),
        )
        .await
        .expect_err("corrupt payload must be rejected");

        assert_eq!(status.code(), Code::Internal);
    }

    #[tokio::test]
    async fn rejects_corrupt_zstd_frame() {
        let payload = b"payload whose zstd magic gets corrupted".repeat(100);
        let mut compressed = compress(&CompressionScheme::Zstd, &payload);
        compressed[0] ^= 0xff;

        let status = decompress_body(
            Some(CompressionScheme::Zstd),
            full_body(grpc_frame(true, &compressed)),
        )
        .await
        .expect_err("corrupt payload must be rejected");

        assert_eq!(status.code(), Code::Internal);
    }

    #[tokio::test]
    async fn rejects_compressed_frame_without_negotiated_scheme() {
        let frame = grpc_frame(true, &gzip_compress(b"data"));

        let status = decompress_body(None, full_body(frame))
            .await
            .expect_err("compressed frame without a negotiated scheme must be rejected");

        assert_eq!(status.code(), Code::Internal);
    }

    #[tokio::test]

    async fn forwards_trailers_after_messages() {
        let payload = b"a compressed message followed by trailers".repeat(100);
        let mut trailers = HeaderMap::new();
        trailers.insert("x-vector-test", HeaderValue::from_static("trailer"));

        let body = StreamBody::new(stream::iter([
            Ok::<_, Infallible>(Frame::data(Bytes::from(grpc_frame(
                true,
                &gzip_compress(&payload),
            )))),
            Ok(Frame::trailers(trailers.clone())),
        ]));

        let out = decompress_body(Some(CompressionScheme::Gzip), body)
            .await
            .expect("decompression must succeed");

        let expected = grpc_frame(false, &payload);
        assert_eq!(out.data, expected);
        assert_eq!(out.bytes_received, expected.len());
        assert_eq!(out.trailers, Some(trailers));
    }

    #[test]
    fn waits_for_destination_before_reading_more_upstream() {
        let frames = identity_frames(3);
        let pulled = Arc::new(AtomicUsize::new(0));
        let source = stream_body(counting_stream(frames.clone(), &pulled));
        let (destination, mut decompressed) = decompressed_body_channel();
        let mut driver = task::spawn(drive_body_decompression(source, destination, None));

        // The destination buffers one message; the driver then waits to forward the second
        // message instead of reading the third chunk.
        assert_pending!(driver.poll());
        assert_eq!(pulled.load(Ordering::SeqCst), 2);
        assert_pending!(driver.poll());
        assert_eq!(pulled.load(Ordering::SeqCst), 2);

        assert_eq!(next_message(&mut decompressed), frames[0]);
        assert!(driver.is_woken());
        assert_pending!(driver.poll());
        assert_eq!(pulled.load(Ordering::SeqCst), 3);

        assert_eq!(next_message(&mut decompressed), frames[1]);
        assert!(driver.is_woken());
        let bytes_received = assert_ready!(driver.poll()).expect("valid messages must decode");
        assert_eq!(bytes_received, frames.iter().map(Vec::len).sum::<usize>());

        assert_eq!(next_message(&mut decompressed), frames[2]);
        let end = decompressed
            .frame()
            .now_or_never()
            .expect("a finished driver must close the destination");
        assert!(end.is_none());
    }

    #[test]

    fn body_error_becomes_grpc_error_response() {
        let source = full_body(grpc_header(false, max_decompressed_size_bytes() + 1));
        let (destination, _decompressed) = decompressed_body_channel();
        let inner = futures::future::pending::<Result<Response<BoxBody>, Infallible>>();
        let mut request = task::spawn(drive_request(
            source,
            destination,
            inner,
            register!(BytesReceived::from(Protocol::from("grpc"))),
            None,
        ));

        let response = assert_ready!(request.poll()).expect("body errors become responses");

        let status = Status::from_header_map(response.headers())
            .expect("the response must carry a gRPC status");
        assert_eq!(status.code(), Code::OutOfRange);
        assert_eq!(
            accept_encoding(&response),
            Some(GRPC_ACCEPT_ENCODING_VALUE.as_str())
        );
    }
}
