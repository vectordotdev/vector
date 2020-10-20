use crate::buffers::Acker;
use bytes::Bytes;
use futures::{ready, Sink};
use pin_project::pin_project;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::AsyncWrite;
use tokio_util::codec::{BytesCodec, FramedWrite};

const MAX_PENDING_ITEMS: usize = 10_000;

#[pin_project]
pub struct AckerBytesSink<T> {
    #[pin]
    inner: FramedWrite<T, BytesCodec>,
    sizes: Vec<usize>,
    acker: Acker,
    on_success: Box<dyn Fn(usize) + Send>,
}

impl<T: AsyncWrite> AckerBytesSink<T> {
    pub fn new(inner: T, acker: Acker, on_success: Box<dyn Fn(usize) + Send>) -> Self {
        Self {
            inner: FramedWrite::new(inner, BytesCodec::new()),
            sizes: Vec::with_capacity(MAX_PENDING_ITEMS),
            acker,
            on_success,
        }
    }

    pub fn ack(&mut self) {
        self.acker.ack(self.sizes.len());
        for size in self.sizes.drain(..) {
            (self.on_success)(size);
        }
    }
}

impl<T> Sink<Bytes> for AckerBytesSink<T>
where
    T: AsyncWrite + std::marker::Unpin,
{
    type Error = <FramedWrite<T, BytesCodec> as Sink<Bytes>>::Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.as_mut().project().sizes.len() == MAX_PENDING_ITEMS {
            if let Err(error) = ready!(self.as_mut().poll_flush(cx)) {
                return Poll::Ready(Err(error));
            }
        }
        self.project().inner.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Bytes) -> Result<(), Self::Error> {
        let pinned = self.project();
        pinned.sizes.push(item.len());
        pinned.inner.start_send(item)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let result = ready!(self.as_mut().project().inner.poll_flush(cx));
        self.as_mut().get_mut().ack();
        Poll::Ready(result)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let result = ready!(self.as_mut().project().inner.poll_close(cx));
        self.as_mut().get_mut().ack();
        Poll::Ready(result)
    }
}
