use std::{
    cmp,
    future::Future,
    io::{self, Write},
    mem,
    pin::Pin,
    sync::LazyLock,
    task::{Context, Poll},
};

use bytes::{Buf, BufMut, BytesMut};
use flate2::write::GzDecoder;
use futures_util::FutureExt;
use http::{HeaderValue, Request, Response};
use hyper::{
    Body,
    body::{HttpBody, Sender},
};
use tokio::{pin, select};
use tonic::{Status, body::BoxBody, metadata::AsciiMetadataValue};
use tower::{Layer, Service};
use vector_lib::internal_event::{
    ByteSize, BytesReceived, InternalEventHandle as _, Protocol, Registered,
};

use crate::internal_events::{GrpcError, GrpcInvalidCompressionSchemeError};

// Every gRPC message has a five byte header:
// - a compressed flag (u8, 0/1 for compressed/decompressed)
// - a length prefix, indicating the number of remaining bytes to read (u32)
const GRPC_MESSAGE_HEADER_LEN: usize = mem::size_of::<u8>() + mem::size_of::<u32>();
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
    fn to_scheme(self) -> Option<CompressionScheme> {
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
    fn from_encoding_header(req: &Request<Body>) -> Result<Option<Self>, Status> {
        req.headers()
            .get(GRPC_ENCODING_HEADER)
            .map(|s| {
                s.to_str().map(|s| s.to_string()).map_err(|_| {
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

enum Decompressor {
    Gzip(Box<GzDecoder<Vec<u8>>>),
    Zstd {
        compressed: Vec<u8>,
        output_buf: Vec<u8>,
    },
}

impl Decompressor {
    fn new(scheme: &CompressionScheme) -> Result<Self, io::Error> {
        // Create the backing buffer for the decompressor and set the compression flag to false (0)
        // and pre-allocate the space for the length prefix, which we'll fill out once we've
        // finalized the decompressor.
        let buf = vec![0; GRPC_MESSAGE_HEADER_LEN];
        match scheme {
            CompressionScheme::Gzip => Ok(Decompressor::Gzip(Box::new(GzDecoder::new(buf)))),
            CompressionScheme::Zstd => Ok(Decompressor::Zstd {
                compressed: Vec::new(),
                output_buf: buf,
            }),
        }
    }

    fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        match self {
            Decompressor::Gzip(d) => d.write_all(data),
            Decompressor::Zstd { compressed, .. } => {
                compressed.extend_from_slice(data);
                Ok(())
            }
        }
    }

    fn finish(self) -> io::Result<Vec<u8>> {
        match self {
            Decompressor::Gzip(d) => (*d).finish(),
            // Decode directly into output_buf to avoid a temporary intermediate Vec that
            // decode_all would produce; peak memory is compressed + decompressed rather than
            // compressed + 2 × decompressed.
            Decompressor::Zstd {
                compressed,
                mut output_buf,
            } => {
                zstd::stream::copy_decode(io::Cursor::new(&compressed), &mut output_buf)?;
                Ok(output_buf)
            }
        }
    }
}

async fn drive_body_decompression(
    mut source: Body,
    mut destination: Sender,
    scheme: Option<CompressionScheme>,
) -> Result<usize, Status> {
    let mut state = State::default();
    let mut buf = BytesMut::new();
    let mut decompressor: Option<Decompressor> = None;
    let mut bytes_received = 0;

    // Drain all message chunks from the body first.
    while let Some(result) = source.data().await {
        let chunk = result.map_err(|_| Status::internal("failed to read from underlying body"))?;
        buf.put(chunk);

        let maybe_message = loop {
            match state {
                State::WaitingForHeader => {
                    // If we don't have enough data yet to even read the gRPC message header, we can't do anything yet.
                    if buf.len() < GRPC_MESSAGE_HEADER_LEN {
                        break None;
                    }

                    // Extract the compressed flag and length prefix.
                    let (is_compressed, message_len) = {
                        let header = &buf[..GRPC_MESSAGE_HEADER_LEN];

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
                        // We skip the header in the buffer because it doesn't matter to the decompressor and we
                        // recreate it anyways.
                        buf.advance(GRPC_MESSAGE_HEADER_LEN);

                        state = State::Decompress {
                            remaining: message_len,
                        };
                    } else {
                        let overall_len = GRPC_MESSAGE_HEADER_LEN + message_len;
                        state = State::Forward { overall_len };
                    }
                }
                State::Forward { overall_len } => {
                    // All we're doing at this point is waiting until we have all the bytes for the current gRPC message
                    // before we emit them to the caller.
                    if buf.len() < overall_len {
                        break None;
                    }

                    // Now that we have all the bytes we need, slice them out of our internal buffer, reset our state,
                    // and hand the message back to the caller.
                    let message = buf.split_to(overall_len).freeze();
                    state = State::WaitingForHeader;

                    bytes_received += overall_len;

                    break Some(message);
                }
                State::Decompress { ref mut remaining } => {
                    if *remaining > 0 {
                        // We're waiting for `remaining` more bytes to feed to the decompressor before we finalize it and
                        // generate our new chunk of data. We might have data in our internal buffer, so try and drain that
                        // first before polling the underlying body for more.
                        let available = buf.len();
                        if available > 0 {
                            // Write the lesser of what the buffer has, or what is remaining for the current message, into
                            // the decompressor. This is _technically_ synchronous but there's really no way to do it
                            // asynchronously since we already have the data, and that's the only asynchronous part.
                            let to_take = cmp::min(available, *remaining);
                            if decompressor.is_none() {
                                decompressor = Some(
                                    Decompressor::new(
                                        scheme.as_ref().unwrap_or(&CompressionScheme::Gzip),
                                    )
                                    .map_err(|_| {
                                        Status::internal("failed to initialize decompressor")
                                    })?,
                                );
                            }
                            let d = decompressor.as_mut().expect("decompressor must be set");
                            if d.write_all(&buf[..to_take]).is_err() {
                                return Err(Status::internal("failed to write to decompressor"));
                            }

                            *remaining -= to_take;
                            buf.advance(to_take);
                        } else {
                            break None;
                        }
                    } else {
                        // We don't need any more data, so consume the decompressor, finalize it by updating the length
                        // prefix, and then pass it back to the caller.
                        let result = decompressor
                            .take()
                            .expect("consumed decompressor when no decompressor was present")
                            .finish();

                        // The only I/O errors that occur during `finish` should be I/O errors from writing to the internal
                        // buffer, but `Vec<T>` is infallible in this regard, so this should be impossible without having
                        // first panicked due to memory exhaustion.
                        let mut buf = result.map_err(|_| {
                            Status::internal(
                                "reached impossible error during decompressor finalization",
                            )
                        })?;
                        bytes_received += buf.len();

                        // Write the length of our decompressed message in the pre-allocated slot for the message's length prefix.
                        let message_len_actual = buf.len() - GRPC_MESSAGE_HEADER_LEN;
                        let message_len = u32::try_from(message_len_actual).map_err(|_| {
                            Status::out_of_range("messages greater than 4GB are not supported")
                        })?;

                        let message_len_bytes = message_len.to_be_bytes();
                        let message_len_slot = &mut buf[1..GRPC_MESSAGE_HEADER_LEN];
                        message_len_slot.copy_from_slice(&message_len_bytes[..]);

                        // Reset our state before returning the decompressed message.
                        state = State::WaitingForHeader;

                        break Some(buf.into());
                    }
                }
            }
        };

        if let Some(message) = maybe_message {
            // We got a decompressed (or passthrough) message chunk, so just forward it to the destination.
            if destination.send_data(message).await.is_err() {
                return Err(Status::internal("destination body abnormally closed"));
            }
        }
    }

    // When we've exhausted all the message chunks, we try sending any trailers that came in on the underlying body.
    let result = source.trailers().await;
    let maybe_trailers =
        result.map_err(|_| Status::internal("error reading trailers from underlying body"))?;
    if let Some(trailers) = maybe_trailers
        && destination.send_trailers(trailers).await.is_err()
    {
        return Err(Status::internal("destination body abnormally closed"));
    }

    Ok(bytes_received)
}

async fn drive_request<F, E>(
    source: Body,
    destination: Sender,
    inner: F,
    bytes_received: Registered<BytesReceived>,
    scheme: Option<CompressionScheme>,
) -> Result<Response<BoxBody>, E>
where
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
                Err(e) => break Ok(e.to_http()),
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
    };

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

impl<S> Service<Request<Body>> for DecompressionAndMetrics<S>
where
    S: Service<Request<Body>, Response = Response<BoxBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: std::fmt::Display,
{
    type Response = Response<BoxBody>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        match CompressionScheme::from_encoding_header(&req) {
            // There was a header for the encoding, but it was either invalid data or a scheme we don't support.
            Err(status) => {
                emit!(GrpcInvalidCompressionSchemeError { status: &status });
                Box::pin(async move { Ok(status.to_http()) })
            }

            // The request either isn't using compression, or it has indicated compression may be used and we know we
            // can support decompression based on the indicated compression scheme... so wrap the body to decompress, if
            // need be, and then track the bytes that flowed through.
            Ok(scheme) => {
                let (destination, decompressed_body) = Body::channel();
                let (mut req_parts, req_body) = req.into_parts();
                // Since this layer owns compression negotiation and is about to hand the
                // inner service a fully decompressed body (with the per-message compressed
                // flag cleared), strip the `grpc-encoding` header so tonic's codegen treats
                // the request as uncompressed and does not try to validate the encoding
                // against any per-service `accept_compressed(..)` configuration.
                if scheme.is_some() {
                    req_parts.headers.remove(GRPC_ENCODING_HEADER);
                }
                let mapped_req = Request::from_parts(req_parts, decompressed_body);

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
