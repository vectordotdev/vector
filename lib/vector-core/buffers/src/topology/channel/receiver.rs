use std::{
    fmt,
    pin::Pin,
    task::{Context, Poll},
};

use futures::Stream;
use pin_project::pin_project;
use tokio::sync::mpsc::Receiver;
use tokio_stream::wrappers::ReceiverStream;

use super::{strategy::StrategyResult, PollStrategy};
use crate::{buffer_usage_data::BufferUsageHandle, Bufferable};

/// Adapter for papering over various receiver backends by providing a [`Stream`] interface.
#[pin_project(project = ProjectedReceiverAdapter)]
pub enum ReceiverAdapter<T> {
    /// A receiver that uses a Tokio MPSC channel.
    Channel(#[pin] ReceiverStream<T>),

    /// A receiver that provides its own [`Stream`] implementation.
    Opaque(Pin<Box<dyn Stream<Item = T> + Send + Sync>>),
}

impl<T> ReceiverAdapter<T>
where
    T: Bufferable,
{
    pub fn channel(rx: Receiver<T>) -> Self {
        ReceiverAdapter::Channel(ReceiverStream::new(rx))
    }

    pub fn opaque<S>(inner: S) -> Self
    where
        S: Stream<Item = T> + Send + Sync + 'static,
    {
        ReceiverAdapter::Opaque(Box::pin(inner))
    }
}

impl<T> fmt::Debug for ReceiverAdapter<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Channel(_) => f.debug_tuple("inner").field(&"Channel").finish(),
            Self::Opaque(_) => f.debug_tuple("inner").field(&"Opaque").finish(),
        }
    }
}

impl<T> Stream for ReceiverAdapter<T>
where
    T: Bufferable,
{
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        match self.project() {
            ProjectedReceiverAdapter::Channel(rx) => rx.poll_next(cx),
            ProjectedReceiverAdapter::Opaque(inner) => inner.as_mut().poll_next(cx),
        }
    }
}

/// A buffer receiver.
///
/// The receiver handles retrieving events from the buffer, regardless of the overall buffer configuration.
///
/// If a buffer was configured to operate in "overflow" mode, then the receiver will be responsible
/// for querying the overflow buffer as well.  The ordering of events when operating in "overflow"
/// is undefined, as the receiver will try to manage polling both its own buffer, as well as the
/// overflow buffer, in order to fairly balance throughput.
#[pin_project]
#[derive(Debug)]
pub struct BufferReceiver<T> {
    #[pin]
    base: ReceiverAdapter<T>,
    overflow: Option<Box<BufferReceiver<T>>>,
    strategy: PollStrategy,
    instrumentation: Option<BufferUsageHandle>,
}

impl<T> BufferReceiver<T> {
    /// Creates a new [`BufferReceiver`] wrapping the given channel receiver.
    pub fn new(base: ReceiverAdapter<T>) -> Self {
        Self {
            base,
            overflow: None,
            strategy: PollStrategy::default(),
            instrumentation: None,
        }
    }

    /// Creates a new [`BufferReceiver`] wrapping the given channel receiver and overflow receiver.
    pub fn with_overflow(base: ReceiverAdapter<T>, overflow: BufferReceiver<T>) -> Self {
        Self {
            base,
            overflow: Some(Box::new(overflow)),
            strategy: PollStrategy::default(),
            instrumentation: None,
        }
    }

    /// Converts this receiver into an overflowing receiver using the given `BufferSender<T>`.
    ///
    /// Note: this resets the internal state of this sender, and so this should not be called except
    /// when initially constructing `BufferSender<T>`.
    #[cfg(test)]
    pub fn switch_to_overflow(&mut self, overflow: BufferReceiver<T>) {
        self.overflow = Some(Box::new(overflow));
    }

    /// Configures this receiver to instrument the items passing through it.
    pub fn with_instrumentation(&mut self, handle: BufferUsageHandle) {
        self.instrumentation = Some(handle);
    }
}

impl<T: Bufferable> Stream for BufferReceiver<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // We want to poll both our base and overflow receivers without waiting for one or the
        // other to entirely drain before checking the other.  This ensures that we're fairly
        // servicing both receivers, and avoiding stalls in one or the other.
        //
        // This is primarily important in situations where an overflow-triggering event has
        // occurred, and is over, and items are flowing through the base receiver.  If we waited to
        // entirely drain the overflow receiver, we might cause another small stall of the pipeline
        // attached to the base receiver.

        let this = self.project();
        let primary = this.base;
        let secondary = this.overflow.as_mut().map(Pin::new);

        this.strategy
            .poll_streams(primary, secondary, cx)
            .map(|result| match result {
                StrategyResult::Primary(i) => {
                    if let Some(handle) = this.instrumentation {
                        handle.increment_sent_event_count_and_byte_size(1, i.size_of() as u64);
                    }
                    Some(i)
                }
                StrategyResult::Secondary(i) => Some(i),
                StrategyResult::Neither => None,
            })
    }
}
