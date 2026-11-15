use std::{
    io::{Error as IoError, IoSlice},
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
    internal_event::{
        ByteSize, InternalEventHandle, Registered, SinkNetworkBytesSent, emit, register,
    },
    json_size::JsonSize,
};

use super::EncodedEvent;
use crate::internal_events::{SocketBytesSent, SocketEventsSent, SocketMode};

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
    inner: FramedWrite<NetworkWriter<T>, BytesCodec>,
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
            inner: FramedWrite::new(
                NetworkWriter {
                    inner,
                    bytes_sent: register(SinkNetworkBytesSent {
                        protocol: socket_mode.as_str().into(),
                    }),
                },
                BytesCodec::new(),
            ),
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

/// Counts completed writes rather than buffered events or acknowledgements. A partial write is
/// still network traffic when a later write/flush fails. TLS streams expose plaintext payload
/// sizes here; record that approximation without claiming to include TLS or TCP overhead.
struct NetworkWriter<T> {
    inner: T,
    bytes_sent: Registered<SinkNetworkBytesSent>,
}

impl<T: AsyncWrite + Unpin> AsyncWrite for NetworkWriter<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, IoError>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(written)) = result {
            this.bytes_sent.emit(ByteSize(written));
        }
        result
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<Result<usize, IoError>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.inner).poll_write_vectored(cx, bufs);
        if let Poll::Ready(Ok(written)) = result {
            this.bytes_sent.emit(ByteSize(written));
        }
        result
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

struct State {
    socket_mode: SocketMode,
    events_total: usize,
    event_bytes: JsonSize,
    bytes_total: usize,
    finalizers: Vec<EventFinalizers>,
}

impl State {
    fn ack(&mut self, status: EventStatus) {
        if self.events_total > 0 {
            for finalizer in std::mem::take(&mut self.finalizers) {
                finalizer.update_status(status);
            }

            if status == EventStatus::Delivered {
                emit(SocketEventsSent {
                    mode: self.socket_mode,
                    count: self.events_total as u64,
                    byte_size: self.event_bytes,
                });
                emit(SocketBytesSent {
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
        self.get_mut().state.ack(EventStatus::Dropped);
    }
}

impl<T> Sink<EncodedEvent<Bytes>> for BytesSink<T>
where
    T: AsyncWrite + Unpin,
{
    type Error = IoError;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.as_mut().project().state.events_total >= MAX_PENDING_ITEMS
            && let Err(error) = ready!(self.as_mut().poll_flush(cx))
        {
            return Poll::Ready(Err(error));
        }

        let inner = self.project().inner;
        <FramedWrite<NetworkWriter<T>, BytesCodec> as Sink<Bytes>>::poll_ready(inner, cx)
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
        match (pinned.shutdown_check)(&mut pinned.inner.get_mut().get_mut().inner) {
            ShutdownCheck::Error(error) => return Poll::Ready(Err(error)),
            ShutdownCheck::Close(reason) => {
                if let Err(error) = ready!(self.as_mut().poll_close(cx)) {
                    return Poll::Ready(Err(error));
                }

                return Poll::Ready(Err(IoError::other(reason)));
            }
            ShutdownCheck::Alive => {}
        }

        let inner = self.as_mut().project().inner;
        let result = ready!(
            <FramedWrite<NetworkWriter<T>, BytesCodec> as Sink<Bytes>>::poll_flush(inner, cx)
        );
        self.as_mut().get_mut().state.ack(match result {
            Ok(()) => EventStatus::Delivered,
            Err(_) => EventStatus::Errored,
        });
        Poll::Ready(result)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let inner = self.as_mut().project().inner;
        let result = ready!(
            <FramedWrite<NetworkWriter<T>, BytesCodec> as Sink<Bytes>>::poll_close(inner, cx)
        );
        self.as_mut().get_mut().state.ack(EventStatus::Dropped);
        Poll::Ready(result)
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::AsyncWriteExt;

    use super::*;
    use crate::test_util::{metrics::counter_value_with_tags, trace_init};

    struct FailingWriter {
        wrote: bool,
    }

    impl AsyncWrite for FailingWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &[u8],
        ) -> Poll<Result<usize, IoError>> {
            let this = self.get_mut();
            if this.wrote {
                Poll::Ready(Err(IoError::other("connection reset")))
            } else {
                this.wrote = true;
                Poll::Ready(Ok(3))
            }
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
            Poll::Ready(Err(IoError::other("flush failed")))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn network_writer_counts_partial_write_even_when_delivery_fails() {
        trace_init();
        let protocol = "network_writer_failure";
        let counter = || {
            counter_value_with_tags(
                "component_sent_network_bytes_total",
                &[("protocol", protocol)],
            )
        };
        let before = counter();
        let mut writer = NetworkWriter {
            inner: FailingWriter { wrote: false },
            bytes_sent: register(SinkNetworkBytesSent {
                protocol: protocol.into(),
            }),
        };
        assert!(writer.write_all(b"some data").await.is_err());
        assert!(writer.flush().await.is_err());
        assert_eq!((counter() - before).to_bits(), 3.0_f64.to_bits());
        assert_eq!(
            counter_value_with_tags("component_sent_bytes_total", &[("protocol", protocol)])
                .to_bits(),
            0.0_f64.to_bits(),
        );
    }
}
