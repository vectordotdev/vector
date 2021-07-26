use futures::Sink;
use std::pin::Pin;
use std::task::{Context, Poll};
use vector_core::event::Event;

pub(crate) struct LogSink {}

impl LogSink {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl Sink<Event> for LogSink {
    type Error = ();

    /// Prepare `LogSink` to receive a new `Event`
    ///
    /// Returns `Poll::Ready` if the sink can receive a new `Event`,
    /// `Poll::Pending` otherwise. A pending response can mean that the `Sink`
    /// has too many in-flight requests active at once.
    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, _event: Event) -> Result<(), Self::Error> {
        // a temporary blackhole
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(())) // << no buffered items remain
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(())) // << no buffered items remain
    }
}
