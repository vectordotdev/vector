use std::{
    io::{Error as IoError, ErrorKind},
    marker::Unpin,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures::{ready, Sink};
use pin_project::{pin_project, pinned_drop};
use tokio::io::AsyncWrite;
use tokio_util::codec::{BytesCodec, FramedWrite};
use vector_buffers::Acker;

use crate::internal_events::{SocketEventsSent, SocketMode};

const MAX_PENDING_ITEMS: usize = 1_000;

pub enum ShutdownCheck {
    Error(IoError),
    Close(&'static str),
    Alive,
}

/// [FramedWrite](https://docs.rs/tokio-util/0.3.1/tokio_util/codec/struct.FramedWrite.html) wrapper.
/// Wrapper acts like [Sink](https://docs.rs/futures/0.3.7/futures/sink/trait.Sink.html) forwarding all
/// calls to `FramedWrite`, but in addition:
/// - Call `shutdown_check` on each `poll_flush`, so we can stop sending data if other side disconnected.
/// - Flush all data on each `poll_ready` if total number of events in queue more than some limit.
/// - Count event size on each `start_send`.
/// - Ack all sent events on successful `poll_flush` and `poll_close` or on `Drop`.
#[pin_project(PinnedDrop)]
pub struct BytesSink<T>
where
    T: AsyncWrite + Unpin,
{
    #[pin]
    inner: FramedWrite<T, BytesCodec>,
    shutdown_check: Box<dyn Fn(&mut T) -> ShutdownCheck + Send>,
    acker: Acker,
    socket_mode: SocketMode,
    events_total: usize,
    bytes_total: usize,
}

impl<T> BytesSink<T>
where
    T: AsyncWrite + Unpin,
{
    pub(crate) fn new(
        inner: T,
        shutdown_check: impl Fn(&mut T) -> ShutdownCheck + Send + 'static,
        acker: Acker,
        socket_mode: SocketMode,
    ) -> Self {
        Self {
            inner: FramedWrite::new(inner, BytesCodec::new()),
            shutdown_check: Box::new(shutdown_check),
            events_total: 0,
            bytes_total: 0,
            acker,
            socket_mode,
        }
    }

    fn ack(&mut self) {
        if self.events_total > 0 {
            self.acker.ack(self.events_total);

            emit!(&SocketEventsSent {
                mode: self.socket_mode,
                count: self.events_total as u64,
                byte_size: self.bytes_total,
            });

            self.events_total = 0;
            self.bytes_total = 0;
        }
    }
}

#[pinned_drop]
impl<T> PinnedDrop for BytesSink<T>
where
    T: AsyncWrite + Unpin,
{
    fn drop(self: Pin<&mut Self>) {
        self.get_mut().ack()
    }
}

impl<T> Sink<Bytes> for BytesSink<T>
where
    T: AsyncWrite + Unpin,
{
    type Error = <FramedWrite<T, BytesCodec> as Sink<Bytes>>::Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if *self.as_mut().project().events_total >= MAX_PENDING_ITEMS {
            if let Err(error) = ready!(self.as_mut().poll_flush(cx)) {
                return Poll::Ready(Err(error));
            }
        }

        self.project().inner.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Bytes) -> Result<(), Self::Error> {
        let pinned = self.project();
        *pinned.events_total += 1;
        *pinned.bytes_total += item.len();
        pinned.inner.start_send(item)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let pinned = self.as_mut().project();
        match (pinned.shutdown_check)(pinned.inner.get_mut().get_mut()) {
            ShutdownCheck::Error(error) => return Poll::Ready(Err(error)),
            ShutdownCheck::Close(reason) => {
                if let Err(error) = ready!(self.as_mut().poll_close(cx)) {
                    return Poll::Ready(Err(error));
                }

                return Poll::Ready(Err(IoError::new(ErrorKind::Other, reason)));
            }
            ShutdownCheck::Alive => {}
        }

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
