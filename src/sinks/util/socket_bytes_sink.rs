use std::{
    fmt,
    io::Error as IoError,
    marker::Unpin,
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

/// Cap on the total encoded size retained in a [`PendingBatch`].
///
/// [`MAX_PENDING_ITEMS`] alone does not bound memory: a single encoded event can be arbitrarily
/// large, so an item-only cap lets a batch of big log records grow without limit while the remote
/// is unavailable.
pub(crate) const MAX_PENDING_BYTES: usize = 4 * 1_024 * 1_024;

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
/// Bounded by both event count ([`MAX_PENDING_ITEMS`]) and total encoded size
/// ([`MAX_PENDING_BYTES`]) so that large records cannot pin an unbounded amount of memory while
/// the remote is unreachable.
///
/// If a sink task is cancelled while events are buffered here, we mark them
/// `Errored` so batch-level acks do not default to `Delivered`.
#[derive(Default)]
pub(crate) struct PendingBatch {
    events: Vec<EncodedEvent<Bytes>>,
    encoded_bytes: usize,
}

impl PendingBatch {
    pub(crate) const fn new() -> Self {
        Self {
            events: Vec::new(),
            encoded_bytes: 0,
        }
    }

    pub(crate) const fn len(&self) -> usize {
        self.events.len()
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Whether collection should stop because either bound has been reached.
    ///
    /// The byte bound is checked after the fact so that a single event larger than
    /// [`MAX_PENDING_BYTES`] is still collected rather than stalling the sink forever.
    pub(crate) const fn is_full(&self) -> bool {
        self.events.len() >= MAX_PENDING_ITEMS || self.encoded_bytes >= MAX_PENDING_BYTES
    }

    pub(crate) fn push(&mut self, encoded: EncodedEvent<Bytes>) {
        self.encoded_bytes = self.encoded_bytes.saturating_add(encoded.item.len());
        self.events.push(encoded);
    }

    /// The first `count` queued events, for feeding into the socket.
    pub(crate) fn head(&self, count: usize) -> &[EncodedEvent<Bytes>] {
        &self.events[..count.min(self.events.len())]
    }

    /// Retire the first `count` events, marking them `Delivered`.
    ///
    /// Called after each successful flush so a partially flushed batch does not have to be
    /// resent from the start on the next reconnect.
    pub(crate) fn ack_delivered(&mut self, count: usize) {
        let count = count.min(self.events.len());
        for encoded in self.events.drain(..count) {
            self.encoded_bytes = self.encoded_bytes.saturating_sub(encoded.item.len());
            encoded.finalizers.update_status(EventStatus::Delivered);
        }
    }
}

impl Drop for PendingBatch {
    fn drop(&mut self) {
        for encoded in self.events.drain(..) {
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

    use super::{
        MAX_PENDING_BYTES, MAX_PENDING_ITEMS, PendingBatch, is_peer_shutdown_error,
        peer_shutdown_io_error,
    };
    use crate::sinks::util::EncodedEvent;

    fn encoded(len: usize) -> EncodedEvent<Bytes> {
        EncodedEvent {
            item: Bytes::from(vec![b'x'; len]),
            finalizers: Default::default(),
            byte_size: len,
            json_byte_size: JsonSize::zero(),
        }
    }

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

    #[test]
    fn pending_batch_is_full_on_byte_bound_before_item_bound() {
        let mut pending = PendingBatch::new();
        let chunk = MAX_PENDING_BYTES / 4;

        for _ in 0..4 {
            assert!(!pending.is_full());
            pending.push(encoded(chunk));
        }

        assert!(pending.is_full());
        assert!(pending.len() < MAX_PENDING_ITEMS);
    }

    #[test]
    fn pending_batch_always_accepts_one_oversized_event() {
        let mut pending = PendingBatch::new();
        assert!(!pending.is_full());
        pending.push(encoded(MAX_PENDING_BYTES * 2));
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn pending_batch_ack_delivered_retires_only_the_flushed_prefix() {
        let (batch, mut delivered) = BatchNotifier::new_with_receiver();
        let (retained_batch, mut retained) = BatchNotifier::new_with_receiver();

        let mut pending = PendingBatch::new();
        pending.push(EncodedEvent {
            finalizers: super::EventFinalizers::new(EventFinalizer::new(batch)),
            ..encoded(MAX_PENDING_BYTES / 2)
        });
        pending.push(EncodedEvent {
            finalizers: super::EventFinalizers::new(EventFinalizer::new(retained_batch)),
            ..encoded(MAX_PENDING_BYTES / 2)
        });
        assert!(pending.is_full());

        pending.ack_delivered(1);

        assert_eq!(delivered.try_recv(), Ok(BatchStatus::Delivered));
        assert_eq!(pending.len(), 1);
        // The freed bytes are returned to the budget so collection can resume.
        assert!(!pending.is_full());
        // The unflushed event stays queued and unfinalized, ready for the next chunk.
        assert!(retained.try_recv().is_err());
    }

    #[test]
    fn pending_batch_head_is_clamped_to_length() {
        let mut pending = PendingBatch::new();
        pending.push(encoded(4));
        assert_eq!(pending.head(MAX_PENDING_ITEMS).len(), 1);
    }
}
