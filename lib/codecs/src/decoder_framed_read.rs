use bytes::BytesMut;
use futures::Stream;
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::AsyncRead;
use tokio_util::codec::{Decoder, FramedRead};

struct DecoderResultWrapper<D> {
    inner: D,
}

impl<D> DecoderResultWrapper<D>
where
    D: Decoder,
{
    const fn new(inner: D) -> Self {
        Self { inner }
    }
}

impl<D> Decoder for DecoderResultWrapper<D>
where
    D: Decoder,
{
    type Item = Result<D::Item, D::Error>;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match self.inner.decode(src) {
            Ok(item) => Ok(item.map(Ok)),
            Err(error) => Ok(Some(Err(error))),
        }
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match self.inner.decode_eof(src) {
            Ok(item) => Ok(item.map(Ok)),
            Err(error) => Ok(Some(Err(error))),
        }
    }
}

/// A `FramedRead` wrapper that keeps decoding after decoder errors.
///
/// `tokio_util::codec::FramedRead` terminates the stream after a decoder error.
/// Vector decoders classify some errors as recoverable, and callers rely on being
/// able to continue after those errors.
pub struct DecoderFramedRead<T, D> {
    inner: FramedRead<T, DecoderResultWrapper<D>>,
}

impl<T, D> DecoderFramedRead<T, D>
where
    T: AsyncRead,
    D: Decoder,
{
    /// Creates a new `DecoderFramedRead` with the given decoder.
    pub fn new(inner: T, decoder: D) -> Self {
        Self {
            inner: FramedRead::new(inner, DecoderResultWrapper::new(decoder)),
        }
    }

    /// Creates a new `DecoderFramedRead` with a specific buffer capacity.
    pub fn with_capacity(inner: T, decoder: D, capacity: usize) -> Self {
        Self {
            inner: FramedRead::with_capacity(inner, DecoderResultWrapper::new(decoder), capacity),
        }
    }

    /// Returns a reference to the underlying I/O stream.
    pub fn get_ref(&self) -> &T {
        self.inner.get_ref()
    }

    /// Returns a mutable reference to the underlying I/O stream.
    pub fn get_mut(&mut self) -> &mut T {
        self.inner.get_mut()
    }

    /// Returns a reference to the internal read buffer.
    pub fn read_buffer(&self) -> &BytesMut {
        self.inner.read_buffer()
    }
}

impl<T, D> Stream for DecoderFramedRead<T, D>
where
    T: AsyncRead,
    D: Decoder,
    D::Error: From<io::Error>,
{
    type Item = Result<D::Item, D::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // SAFETY: We only project pinning from `self` to the `inner` field and
        // never move `inner` after pinning.
        let inner = unsafe { self.map_unchecked_mut(|this| &mut this.inner) };
        match inner.poll_next(cx) {
            Poll::Ready(Some(Ok(Ok(item)))) => Poll::Ready(Some(Ok(item))),
            Poll::Ready(Some(Ok(Err(error)))) => Poll::Ready(Some(Err(error))),
            Poll::Ready(Some(Err(error))) => Poll::Ready(Some(Err(error.into()))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
