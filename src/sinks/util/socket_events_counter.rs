use crate::{
    buffers::Acker,
    internal_events::{SocketEventsSent, SocketMode},
    Event,
};
use bytes::Bytes;
use futures::{ready, Sink};
use pin_project::{pin_project, pinned_drop};
use std::{
    io::{Error as IoError, ErrorKind},
    marker::Unpin,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};
use tokio::io::AsyncWrite;
use tokio_util::codec::{BytesCodec, FramedWrite};

const MAX_PENDING_ITEMS: usize = 1_000;

/// Count number of encoded events and ack on request.
pub struct EventsCounter {
    sizes: Mutex<Vec<usize>>,
    acker: Acker,
    encode_event_inner: Box<dyn Fn(Event) -> Option<Bytes> + Send + Sync>,
    socket_mode: SocketMode,
}

impl EventsCounter {
    pub(crate) fn new(
        acker: Acker,
        encode_event: impl Fn(Event) -> Option<Bytes> + Send + Sync + 'static,
        socket_mode: SocketMode,
    ) -> Self {
        Self {
            sizes: Mutex::new(Vec::with_capacity(MAX_PENDING_ITEMS)),
            acker,
            encode_event_inner: Box::new(encode_event),
            socket_mode,
        }
    }

    pub fn encode_event(&self, event: Event) -> Option<Result<Bytes, std::io::Error>> {
        let mut sizes = self.sizes.lock().unwrap();
        let bytes = (self.encode_event_inner)(event);
        sizes.push(bytes.as_ref().map_or(0, |bytes| bytes.len()));
        bytes.map(Ok)
    }

    fn is_full(&self) -> bool {
        let sizes = self.sizes.lock().unwrap();
        sizes.len() >= MAX_PENDING_ITEMS
    }

    fn ack(&self, count: usize) {
        let mut sizes = self.sizes.lock().unwrap();
        assert!(
            count <= sizes.len(),
            "can not ack more then consumed events"
        );

        self.acker.ack(count);
        let byte_size = sizes.drain(..count).sum();

        emit!(SocketEventsSent {
            mode: self.socket_mode,
            count: count as u64,
            byte_size,
        })
    }

    fn ack_rest(&self) {
        let mut sizes = self.sizes.lock().unwrap();
        self.acker.ack(sizes.len());
        sizes.clear();
    }
}

pub enum ShutdownCheck {
    Error(IoError),
    Close(&'static str),
    Alive,
}

/// [FramedWrite](https://docs.rs/tokio-util/0.3.1/tokio_util/codec/struct.FramedWrite.html) wrapper.
/// Wrapper acts like [Sink](https://docs.rs/futures/0.3.7/futures/sink/trait.Sink.html) forwarding all
/// calls to `FramedWrite`, but in addition:
/// - Call `shutdown_check` on each `poll_ready`, so we able stop data sending if other side disconnected.
/// - Flush all data on each `poll_ready` if total number of events in queue more than some limit.
/// - Add event size to queue on each `start_send`.
/// - Ack all sent events on successful `poll_flush` and `poll_close`.
/// - Ack rest of encoded events on `Drop`.
#[pin_project(PinnedDrop)]
pub struct BytesSink<T> {
    #[pin]
    inner: FramedWrite<T, BytesCodec>,
    shutdown_check: Box<dyn Fn(&mut T) -> ShutdownCheck + Send>,
    events_count: usize,
    events_counter: Arc<EventsCounter>,
}

impl<T> BytesSink<T>
where
    T: AsyncWrite,
{
    pub fn new(
        inner: T,
        shutdown_check: impl Fn(&mut T) -> ShutdownCheck + Send + 'static,
        events_counter: Arc<EventsCounter>,
    ) -> Self {
        Self {
            inner: FramedWrite::new(inner, BytesCodec::new()),
            shutdown_check: Box::new(shutdown_check),
            events_count: 0,
            events_counter,
        }
    }

    pub fn get_ref(&self) -> &FramedWrite<T, BytesCodec> {
        &self.inner
    }

    fn ack(&mut self) {
        self.events_counter.ack(self.events_count);
        self.events_count = 0;
    }
}

#[pinned_drop]
impl<T> PinnedDrop for BytesSink<T> {
    fn drop(self: Pin<&mut Self>) {
        self.project().events_counter.ack_rest()
    }
}

impl<T> Sink<Bytes> for BytesSink<T>
where
    T: AsyncWrite + Unpin,
{
    type Error = <FramedWrite<T, BytesCodec> as Sink<Bytes>>::Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.as_mut().project().events_counter.is_full() {
            if let Err(error) = ready!(self.as_mut().poll_flush(cx)) {
                return Poll::Ready(Err(error));
            }
        }

        self.project().inner.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Bytes) -> Result<(), Self::Error> {
        let pinned = self.project();
        *pinned.events_count += 1;
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
