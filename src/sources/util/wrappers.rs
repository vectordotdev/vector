use bytes::BytesMut;
use futures::Stream;
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio_util::codec::Decoder;
use vector_lib::codecs::DecoderFramedRead;

use pin_project::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub trait AfterReadExt {
    fn after_read<F>(self, after_read: F) -> AfterRead<Self, F>
    where
        Self: Sized;
}

impl<T: AsyncRead + AsyncWrite> AfterReadExt for T {
    fn after_read<F>(self, after_read: F) -> AfterRead<Self, F> {
        AfterRead::new(self, after_read)
    }
}

/// This wraps the inner socket and emits `BytesReceived` with the
/// actual number of bytes read before handling framing.
#[pin_project]
pub struct AfterRead<T, F> {
    #[pin]
    inner: T,
    after_read: F,
}

impl<T, F> AfterRead<T, F> {
    pub const fn new(inner: T, after_read: F) -> Self {
        Self { inner, after_read }
    }

    #[cfg(feature = "listenfd")]
    pub const fn get_ref(&self) -> &T {
        &self.inner
    }

    #[cfg(all(unix, feature = "sources-utils-net-unix"))]
    pub const fn get_mut_ref(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<T: AsyncRead, F> AsyncRead for AfterRead<T, F>
where
    F: Fn(usize),
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<tokio::io::Result<()>> {
        let before = buf.filled().len();
        let this = self.project();
        let result = this.inner.poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = result {
            (this.after_read)(buf.filled().len() - before);
        }
        result
    }
}

impl<T: AsyncWrite, F> AsyncWrite for AfterRead<T, F> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        self.project().inner.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.project().inner.poll_shutdown(cx)
    }
}

pub enum DecoderError<E> {
    IO(io::Error),
    Other(E),
}

impl<E> DecoderError<E>
where
    E: From<io::Error>,
{
    fn into_inner(self) -> E {
        match self {
            DecoderError::IO(e) => E::from(e),
            DecoderError::Other(e) => e,
        }
    }
}

impl<E> From<io::Error> for DecoderError<E> {
    fn from(e: io::Error) -> Self {
        DecoderError::IO(e)
    }
}

pub struct LenientFramedReadDecoder<D> {
    inner: D,
}

impl<D> LenientFramedReadDecoder<D>
where
    D: Decoder,
{
    pub const fn new(inner: D) -> Self {
        Self { inner }
    }
}

impl<D> Decoder for LenientFramedReadDecoder<D>
where
    D: Decoder,
    D::Error: From<io::Error>,
{
    type Item = D::Item;
    type Error = DecoderError<D::Error>;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.inner.decode(src).map_err(DecoderError::Other)
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.inner.decode_eof(src).map_err(DecoderError::Other)
    }
}

/// A wrapper around an `FramedRead` that silently ignores `ConnectionReset`
/// errors if the frame buffer is empty.
#[pin_project]
pub struct LenientFramedRead<T, D> {
    #[pin]
    inner: DecoderFramedRead<T, LenientFramedReadDecoder<D>>,
}

impl<T, D> LenientFramedRead<T, D>
where
    T: AsyncRead,
    D: Decoder,
{
    /// Creates a new `LenientFramedRead` with the given `decoder`.
    pub fn new(inner: T, decoder: D) -> Self {
        Self {
            inner: DecoderFramedRead::new(inner, LenientFramedReadDecoder::new(decoder)),
        }
    }

    /// Returns a reference to the underlying I/O stream wrapped by
    /// `FramedRead`.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn get_ref(&self) -> &T {
        self.inner.get_ref()
    }

    /// Returns a mutable reference to the underlying I/O stream wrapped by
    /// `FramedRead`.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn get_mut(&mut self) -> &mut T {
        self.inner.get_mut()
    }
}

impl<T, D> Stream for LenientFramedRead<T, D>
where
    T: AsyncRead + Unpin,
    D: Decoder,
{
    type Item = Result<D::Item, D::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        match this.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(Err(DecoderError::IO(e))))
                if e.kind() == io::ErrorKind::ConnectionReset =>
            {
                let buffer = this.inner.read_buffer();

                if buffer.is_empty() {
                    // Clean RST - no partial data, treat as EOF
                    Poll::Ready(None)
                } else {
                    // Partial frame in buffer
                    Poll::Ready(Some(Err(D::Error::from(e))))
                }
            }
            other => other.map_err(|e| e.into_inner()),
        }
    }
}
