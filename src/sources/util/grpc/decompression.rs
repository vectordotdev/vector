use std::{convert::Infallible, task::{Context, Poll}, pin::Pin, mem};

use std::future::Future;
use bytes::{BytesMut, Bytes, BufMut};
use flate2::write::GzDecoder;
use futures::ready;
use http::{Request, Response};
use hyper::{Body, body::{Sender, HttpBody}};
use pin_project::pin_project;
use tonic::body::BoxBody;
use tower::{Layer, Service};

// Every gRPC message has a five byte header:
// - a compressed flag (u8, 0/1 for compressed/decompressed)
// - a length prefix, indicating the number of remaining bytes to read (u32)
const GRPC_MESSAGE_HEADER_LEN: usize = mem::size_of::<u8>() + mem::size_of::<u32>();

enum State {
    WaitingForHeader,
    Forward { overall_len: usize },
    Decompress { remaining: usize },
}

impl State {
    fn reset(&mut self) {
        *self = Self::default();
    }
}

impl Default for State {
    fn default() -> Self {
        Self::WaitingForHeader
    }
}

#[pin_project]
struct DecompressingBody {
    state: State,
    buf: BytesMut,
    decompressor: GzDecoder<Vec<u8>>,
    #[pin]
    inner: Body,
}

impl From<Body> for DecompressingBody {
    fn from(inner: Body) -> Self {
        Self {
            state: State::default(),
            buf: BytesMut::new(),
            decompressor: GzDecoder::new(Vec::new()),
            inner,
        }
    }
}

impl HttpBody for DecompressingBody {
    type Data = Bytes;

    type Error = hyper::Error;

    fn poll_data(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        let this = self.project();

        loop {
            let state = this.state;
            match state {
                State::WaitingForHeader => {
                    // We're waiting for enough bytes in our internal buffer to parse our a gRPC message header. Check to
                    // see if we need to poll the underlying body for more: remember, we might have bytes left over from the
                    // last poll, so we check that first.
                    while this.buf.len() < GRPC_MESSAGE_HEADER_LEN {
                        match ready!(this.inner.poll_data(cx)) {
                            // If we got some data, extend our internal buffer with it.
                            Some(Ok(chunk)) => this.buf.put(chunk),
                            // Our underlying body hit an error during the read, so propagate that back to the caller.
                            Some(Err(e)) => return Poll::Ready(Some(Err(e))),
                            None => if self.buf.is_empty() {
                                // We aren't mid-read on a message, and the underlying body is reporting EOF, basically,
                                // so that means we're also done.
                                return Poll::Ready(None)
                            } else {
                                // TODO: This should actually be an error, not a panic, otherwise a silly request could just
                                // totally ruin our day.
                                panic!("waiting for next message header with non-empty buffer but underlying stream hit EOF");
                            }
                        }
                    }

                    // Extract the compressed flag and length prefix.
                    let (is_compressed, message_len) = {
                        let header = &this.buf[..GRPC_MESSAGE_HEADER_LEN];

                        let message_len_raw: u32 = header[1..].try_into()
                            .map(u32::from_be_bytes)
                            .expect("there must be four bytes remaining in the header slice");
                        let message_len = message_len_raw.try_into().expect("Vector does not support 16-bit platforms");

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
                        let overall_len = GRPC_MESSAGE_HEADER_LEN + message_len;
                        *state = State::Forward { overall_len };

                    } else {
                        *state = State::Decompress { remaining: message_len };
                    }
                },
                State::Forward { overall_len } => {
                    // All we're doing at this point is waiting until we have all the bytes for the current gRPC message
                    // before we emit them to the caller.
                    while this.buf.len() < *overall_len {
                        match ready!(this.inner.poll_data(cx)) {
                            // If we got some data, extend our internal buffer with it.
                            Some(Ok(chunk)) => this.buf.put(chunk),
                            // Our underlying body hit an error during the read, so propagate that back to the caller.
                            Some(Err(e)) => return Poll::Ready(Some(Err(e))),
                            // We're mid-read on a message, so this is definitely no bueno.
                            //
                            // TODO: This should actually be an error, not a panic, otherwise a silly request could just
                            // totally ruin our day.
                            None => panic!("waiting for remainder of uncompressed message with non-empty buffer but underlying stream hit EOF"),
                        }
                    }

                    // Now that we have all the bytes we need, slice them out of our internal buffer, reset our state,
                    // and hand the chunk back to the caller.
                    let chunk = this.buf.split_to(*overall_len).freeze();
                    state.reset();

                    return Poll::Ready(Some(Ok(chunk)))
                },
                State::Decompress { remaining } => todo!(),
            }
        }
    }

    fn poll_trailers(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Option<headers::HeaderMap>, Self::Error>> {
        let this = self.project();
        this.inner.poll_trailers(cx)
    }
}

#[pin_project(project = ResponseFutureProj)]
pub enum ResponseFuture<F> {
    /// There was an error with the request. (i.e. an unsupported compression scheme)
    Error(Option<Response<BoxBody>>),
    /// Compression is indicated by the request, but can still be controlled at the per-message level, so we'll process
    /// every message individually, fast pathing them to the underlying service when not compressed, and decompressing
    /// them first otherwise.
    MaybeCompressed {
        body: DecompressingBody,
        destination: Sender,
        #[pin]
        inner: F,
    },
    // No compression is indicated by the request, so we simply drive the underlying service.
    Passthrough {
        #[pin]
        inner: F,
    }
}

impl<F> Future for ResponseFuture<F>
where
    F: Future<Output = Result<Response<BoxBody>, Infallible>>,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project() {
            // When the request had an error (i.e. an unsupported compression scheme) we immediately return a response.
            ResponseFutureProj::Error(maybe_err_response) => {
                let err_response = maybe_err_response.take().expect("cannot poll Future after completion");
                Poll::Ready(Ok(err_response))
            },
            // In passthrough mode, all we do is drive the future from the underlying service.
            ResponseFutureProj::Passthrough { inner } => inner.poll(cx),
            ResponseFutureProj::MaybeCompressed { body, destination, inner } => {

            },
        }
    }
}

pub struct GrpcGzipDecompression<S> {
	inner: S,
}

impl<S> Service<Request<Body>> for GrpcGzipDecompression<S>
where
	S: Service<Request<Body>, Response = Response<BoxBody>, Error = Infallible> + Clone + Send + 'static,
	S::Future: Send + 'static,
{
    type Response = Response<BoxBody>;

    type Error = Infallible;

    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        if check_maybe_compressed(&req) {
            // The request indicates that compression _may_ be present, but could be off at a per-message level, so we
            // have to go through the notions of checking each message and cannot use the fast path.
            let (destination, decompressed_body) = Body::channel();
            let (req_parts, req_body) = req.into_parts();
            let mapped_req = Request::from_parts(req_parts, decompressed_body);

            let inner = self.inner.call(mapped_req);

            ResponseFuture::MaybeCompressed {
                body: req_body.into(),
                destination,
                inner
            }
        } else {
            // No compression is indicated for this request, so we can forward directly to the underlying service as-is.
            let inner = self.inner.call(req);
            ResponseFuture::Passthrough { inner }
        }
    }
}

fn check_maybe_compressed(req: &Request<Body>) -> bool {
    todo!()
}

#[derive(Clone, Default)]
pub struct GrpcGzipDecompressionLayer;

impl<S> Layer<S> for GrpcGzipDecompressionLayer {
    type Service = GrpcGzipDecompression<S>;

    fn layer(&self, inner: S) -> Self::Service {
        GrpcGzipDecompression {
            inner,
        }
    }
}
