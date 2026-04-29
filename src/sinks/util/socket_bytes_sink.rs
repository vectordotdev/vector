use std::{
    fmt,
    io::Error as IoError,
    marker::Unpin,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll, ready},
};

use bytes::Bytes;
use futures::Sink;
use pin_project::{pin_project, pinned_drop};
use tokio::io::AsyncWrite;
use tokio_util::codec::{BytesCodec, FramedWrite};
use vector_lib::{
    finalization::{EventFinalizers, EventStatus},
    json_size::JsonSize,
};

use super::EncodedEvent;
use crate::internal_events::{SocketBytesSent, SocketEventsSent, SocketMode};

pub(crate) const MAX_PENDING_ITEMS: usize = 1_000;

pub enum ShutdownCheck {
    Error(IoError),
    Close(&'static str),
    Alive,
}

#[derive(Debug)]
struct PeerShutdownError {
    reason: &'static str,
}

impl fmt::Display for PeerShutdownError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason)
    }
}

impl std::error::Error for PeerShutdownError {}

pub(crate) fn peer_shutdown_io_error(reason: &'static str) -> IoError {
    IoError::new(
        std::io::ErrorKind::ConnectionAborted,
        PeerShutdownError { reason },
    )
}

pub(crate) fn is_peer_shutdown_error(error: &IoError) -> bool {
    error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<PeerShutdownError>())
        .is_some()
}

/// [FramedWrite](https://docs.rs/tokio-util/0.3.1/tokio_util/codec/struct.FramedWrite.html) wrapper.
/// Wrapper acts like [Sink](https://docs.rs/futures/0.3.7/futures/sink/trait.Sink.html) forwarding all
/// calls to `FramedWrite`, but in addition:
/// - Call `shutdown_check` at the start of each buffered batch (on `poll_ready` when the
///   pending count is zero) and on each `poll_flush`, so we can stop sending if the other side
///   disconnected.
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
    state: State,
}

impl<T> BytesSink<T>
where
    T: AsyncWrite + Unpin,
{
    pub(crate) fn new(
        inner: T,
        shutdown_check: impl Fn(&mut T) -> ShutdownCheck + Send + 'static,
        socket_mode: SocketMode,
    ) -> Self {
        Self {
            inner: FramedWrite::new(inner, BytesCodec::new()),
            shutdown_check: Box::new(shutdown_check),
            state: State {
                events_total: 0,
                event_bytes: JsonSize::zero(),
                bytes_total: 0,
                socket_mode,
                finalizers: Vec::new(),
            },
        }
    }
}

struct State {
    socket_mode: SocketMode,
    events_total: usize,
    event_bytes: JsonSize,
    bytes_total: usize,
    finalizers: Vec<EventFinalizers>,
}

/// In-memory resend queue for socket stream sinks.
///
/// If a sink task is cancelled while events are buffered here, we mark them
/// `Errored` so batch-level acks do not default to `Delivered`.
#[derive(Default)]
pub(crate) struct PendingBatch(Vec<EncodedEvent<Bytes>>);

impl PendingBatch {
    pub(crate) fn new() -> Self {
        Self(Vec::new())
    }
}

impl Deref for PendingBatch {
    type Target = Vec<EncodedEvent<Bytes>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PendingBatch {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for PendingBatch {
    fn drop(&mut self) {
        for encoded in self.0.drain(..) {
            encoded.finalizers.update_status(EventStatus::Errored);
        }
    }
}

impl State {
    fn ack(&mut self, status: EventStatus) {
        if self.events_total > 0 {
            for finalizer in std::mem::take(&mut self.finalizers) {
                finalizer.update_status(status);
            }

            if status == EventStatus::Delivered {
                emit!(SocketEventsSent {
                    mode: self.socket_mode,
                    count: self.events_total as u64,
                    byte_size: self.event_bytes,
                });
                emit!(SocketBytesSent {
                    mode: self.socket_mode,
                    byte_size: self.bytes_total,
                });
            }

            self.events_total = 0;
            self.event_bytes = JsonSize::zero();
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
        self.get_mut().state.ack(EventStatus::Dropped)
    }
}

impl<T> Sink<EncodedEvent<Bytes>> for BytesSink<T>
where
    T: AsyncWrite + Unpin,
{
    type Error = <FramedWrite<T, BytesCodec> as Sink<Bytes>>::Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.as_mut().project().state.events_total == 0 {
            // Detect peer shutdown before accepting the first item in a new batch so we avoid
            // queuing fresh data while the peer is already gone.
            let close_reason = {
                let pinned = self.as_mut().project();
                match (pinned.shutdown_check)(pinned.inner.get_mut().get_mut()) {
                    ShutdownCheck::Error(error) => return Poll::Ready(Err(error)),
                    ShutdownCheck::Close(reason) => Some(reason),
                    ShutdownCheck::Alive => None,
                }
            };
            if let Some(reason) = close_reason {
                // Close the transport only; do not use `BytesSink::poll_close`, which acks
                // `Dropped` and would finalize in-flight events while the TCP sink may reconnect
                // and retry.
                let inner = self.as_mut().project().inner;
                if let Err(error) = ready!(<FramedWrite<T, BytesCodec> as Sink<Bytes>>::poll_close(
                    inner, cx
                )) {
                    return Poll::Ready(Err(error));
                }
                self.as_mut().get_mut().state.ack(EventStatus::Errored);
                return Poll::Ready(Err(peer_shutdown_io_error(reason)));
            }
        }

        if self.as_mut().project().state.events_total >= MAX_PENDING_ITEMS
            && let Err(error) = ready!(self.as_mut().poll_flush(cx))
        {
            return Poll::Ready(Err(error));
        }

        let inner = self.project().inner;
        <FramedWrite<T, BytesCodec> as Sink<Bytes>>::poll_ready(inner, cx)
    }

    fn start_send(self: Pin<&mut Self>, item: EncodedEvent<Bytes>) -> Result<(), Self::Error> {
        let pinned = self.project();
        pinned.state.finalizers.push(item.finalizers);
        pinned.state.events_total += 1;
        pinned.state.event_bytes += item.json_byte_size;
        pinned.state.bytes_total += item.item.len();

        let result = pinned.inner.start_send(item.item);
        if result.is_err() {
            pinned.state.ack(EventStatus::Errored);
        }
        result
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let pinned = self.as_mut().project();
        match (pinned.shutdown_check)(pinned.inner.get_mut().get_mut()) {
            ShutdownCheck::Error(error) => return Poll::Ready(Err(error)),
            ShutdownCheck::Close(reason) => {
                let inner = self.as_mut().project().inner;
                if let Err(error) = ready!(<FramedWrite<T, BytesCodec> as Sink<Bytes>>::poll_close(
                    inner, cx
                )) {
                    return Poll::Ready(Err(error));
                }
                self.as_mut().get_mut().state.ack(EventStatus::Errored);
                return Poll::Ready(Err(peer_shutdown_io_error(reason)));
            }
            ShutdownCheck::Alive => {}
        }

        let inner = self.as_mut().project().inner;
        let result = ready!(<FramedWrite<T, BytesCodec> as Sink<Bytes>>::poll_flush(
            inner, cx
        ));
        self.as_mut().get_mut().state.ack(match result {
            Ok(_) => EventStatus::Delivered,
            Err(_) => EventStatus::Errored,
        });
        Poll::Ready(result)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let inner = self.as_mut().project().inner;
        let result = ready!(<FramedWrite<T, BytesCodec> as Sink<Bytes>>::poll_close(
            inner, cx
        ));
        self.as_mut().get_mut().state.ack(EventStatus::Dropped);
        Poll::Ready(result)
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use bytes::Bytes;
    use vector_lib::event::{BatchNotifier, BatchStatus, EventFinalizer};
    use vector_lib::json_size::JsonSize;

    use super::{PendingBatch, is_peer_shutdown_error, peer_shutdown_io_error};
    use crate::sinks::util::EncodedEvent;

    #[test]
    fn detects_typed_peer_shutdown_error() {
        let error = peer_shutdown_io_error("ShutdownCheck::Close");
        assert!(is_peer_shutdown_error(&error));
    }

    #[test]
    fn ignores_non_peer_shutdown_error() {
        let error = io::Error::other("not peer shutdown");
        assert!(!is_peer_shutdown_error(&error));
    }

    #[test]
    fn pending_batch_drop_marks_finalizers_errored() {
        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let finalizers = super::EventFinalizers::new(EventFinalizer::new(batch));

        {
            let mut pending = PendingBatch::new();
            pending.push(EncodedEvent {
                item: Bytes::from_static(b"test"),
                finalizers,
                byte_size: 0,
                json_byte_size: JsonSize::zero(),
            });
        }

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Errored));
    }
}
