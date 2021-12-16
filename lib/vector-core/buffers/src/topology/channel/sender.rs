use std::{
    fmt,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{ready, Sink};
use pin_project::pin_project;
use tokio::sync::mpsc::Sender;

use super::poll_sender::PollSender;
use crate::{buffer_usage_data::BufferUsageHandle, Bufferable, WhenFull};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum SendState {
    // This sender should drop the next item it receives.
    DropNext,
    // The base sender is ready to be sent an item.
    BaseReady,
    // The overflow sender is ready to be sent an item.
    OverflowReady,
    // Default state.
    Idle,
}

impl SendState {
    fn is_ready(self) -> bool {
        matches!(self, SendState::BaseReady | SendState::OverflowReady)
    }
}

// Some type-level tomfoolery to have a trait that represents a `Sink` that can be cloned.
/// A [`Sink`] that can be cloned.
///
/// Required due to limitations around using non-auto traits in trait signatures.  If your [`Sink`]
/// implementation is also `Clone`, then you are covered by the blanket trait implementation.
pub trait CloneableSink<Item, E>: Sink<Item, Error = E> + Send + dyn_clone::DynClone {}

impl<T, Item, E> CloneableSink<Item, E> for T where T: Sink<Item, Error = E> + Send + Clone {}

dyn_clone::clone_trait_object!(<T, E> CloneableSink<T, E>);

/// Adapter for papering over various sender backends by providing a [`Sink`] interface.
#[pin_project(project = ProjectedSenderAdapter)]
pub enum SenderAdapter<T> {
    /// A sender that uses a Tokio MPSC channel.
    Channel(PollSender<T>),

    /// A sender that provides its own [`Sink`] implementation.
    Opaque(Pin<Box<dyn CloneableSink<T, ()>>>),
}

impl<T> SenderAdapter<T>
where
    T: Bufferable,
{
    pub fn channel(tx: Sender<T>) -> Self {
        SenderAdapter::Channel(PollSender::new(tx))
    }

    pub fn opaque<S>(inner: S) -> Self
    where
        S: CloneableSink<T, ()> + 'static,
    {
        SenderAdapter::Opaque(Box::pin(inner))
    }

    pub fn capacity(&self) -> Option<usize> {
        match self {
            Self::Channel(tx) => tx.get_ref().map(Sender::capacity),
            Self::Opaque(_) => None,
        }
    }
}

impl<T> Clone for SenderAdapter<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Channel(tx) => Self::Channel(tx.clone()),
            Self::Opaque(sink) => Self::Opaque(sink.clone()),
        }
    }
}

impl<T> fmt::Debug for SenderAdapter<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Channel(_) => f.debug_tuple("inner").field(&"Channel").finish(),
            Self::Opaque(_) => f.debug_tuple("inner").field(&"Opaque").finish(),
        }
    }
}

impl<T> Sink<T> for SenderAdapter<T>
where
    T: Bufferable,
{
    type Error = ();

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.project() {
            ProjectedSenderAdapter::Channel(tx) => tx.poll_reserve(cx).map_err(|_| ()),
            ProjectedSenderAdapter::Opaque(inner) => inner.as_mut().poll_ready(cx),
        }
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        match self.project() {
            ProjectedSenderAdapter::Channel(tx) => tx.start_send(item).map_err(|_| ()),
            ProjectedSenderAdapter::Opaque(inner) => inner.as_mut().start_send(item),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.project() {
            // There's nothing to actually flush when using `PollSender<T>`.
            ProjectedSenderAdapter::Channel(_) => Poll::Ready(Ok(())),
            ProjectedSenderAdapter::Opaque(inner) => inner.as_mut().poll_flush(cx),
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.project() {
            ProjectedSenderAdapter::Channel(tx) => {
                if !tx.is_closed() {
                    tx.close();
                }
                Poll::Ready(Ok(()))
            }
            ProjectedSenderAdapter::Opaque(inner) => inner.as_mut().poll_close(cx),
        }
    }
}

/// A buffer sender.
///
/// The sender handles sending events into the buffer, as well as the behavior around handling
/// events when the internal channel is full.
///
/// When creating a buffer sender/receiver pair, callers can specify the "when full" behavior of the
/// sender.  This controls how events are handled when the internal channel is full.  Three modes
/// are possible:
/// - block
/// - drop newest
/// - overflow
///
/// In "block" mode, callers are simply forced to wait until the channel has enough capacity to
/// accept the event.  In "drop newest" mode, any event being sent when the channel is full will be
/// dropped and proceed no further. In "overflow" mode, events will be sent to another buffer
/// sender.  Callers can specify the overflow sender to use when constructing their buffers initially.
#[pin_project]
#[derive(Debug)]
pub struct BufferSender<T> {
    #[pin]
    base: SenderAdapter<T>,
    base_flush: bool,
    #[pin]
    overflow: Option<Box<BufferSender<T>>>,
    overflow_flush: bool,
    state: SendState,
    when_full: WhenFull,
    instrumentation: Option<BufferUsageHandle>,
}

impl<T> BufferSender<T> {
    /// Creates a new [`BufferSender`] wrapping the given channel sender.
    pub fn new(base: SenderAdapter<T>, when_full: WhenFull) -> Self {
        Self {
            base,
            base_flush: false,
            overflow: None,
            overflow_flush: false,
            state: SendState::Idle,
            when_full,
            instrumentation: None,
        }
    }

    /// Creates a new [`BufferSender`] wrapping the given channel sender and overflow sender.
    pub fn with_overflow(base: SenderAdapter<T>, overflow: BufferSender<T>) -> Self {
        Self {
            base,
            base_flush: false,
            overflow: Some(Box::new(overflow)),
            overflow_flush: false,
            state: SendState::Idle,
            when_full: WhenFull::Overflow,
            instrumentation: None,
        }
    }

    /// Converts this sender into an overflowing sender using the given `BufferSender<T>`.
    ///
    /// Note: this resets the internal state of this sender, and so this should not be called except
    /// when initially constructing `BufferSender<T>`.
    #[cfg(test)]
    pub fn switch_to_overflow(&mut self, overflow: BufferSender<T>) {
        self.overflow = Some(Box::new(overflow));
        self.when_full = WhenFull::Overflow;
        self.state = SendState::Idle;
        self.base_flush = false;
        self.overflow_flush = false;
    }

    /// Configures this sender to instrument the items passing through it.
    pub fn with_instrumentation(&mut self, handle: BufferUsageHandle) {
        self.instrumentation = Some(handle);
    }
}

impl<T: Bufferable> BufferSender<T> {
    #[cfg(test)]
    pub(crate) fn get_base_ref(&self) -> &SenderAdapter<T> {
        &self.base
    }

    #[cfg(test)]
    pub(crate) fn get_overflow_ref(&self) -> Option<&BufferSender<T>> {
        self.overflow.as_ref().map(AsRef::as_ref)
    }
}

impl<T> Clone for BufferSender<T> {
    fn clone(&self) -> Self {
        Self {
            base: self.base.clone(),
            base_flush: false,
            overflow: self.overflow.clone(),
            overflow_flush: false,
            state: SendState::Idle,
            when_full: self.when_full,
            instrumentation: self.instrumentation.clone(),
        }
    }
}

impl<T: Bufferable> Sink<T> for BufferSender<T> {
    type Error = ();

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.project();

        // For whatever reason, the caller is calling `poll_ready` again after a successful previous
        // call.  Since we already know we're ready, and `start_send` has not yet been called, we
        // can simply short circuit here and return that we're (still) ready.
        if this.state.is_ready() {
            return Poll::Ready(Ok(()));
        }

        let (result, next_state) = match this.base.poll_ready(cx) {
            Poll::Ready(result) => match result {
                // We reserved a sending slot in the base channel.
                Ok(()) => (Poll::Ready(Ok(())), SendState::BaseReady),
                // Base sender's underlying channel is closed.
                Err(e) => (Poll::Ready(Err(e)), SendState::Idle),
            },
            // Our base sender was not able to immediately reserve a sending slot.
            Poll::Pending => match this.when_full {
                // We need to block.  Nothing else to do, as the base sender will notify us when
                // there's capacity to do the send.
                WhenFull::Block => (Poll::Pending, SendState::Idle),
                // We need to drop the next item.  We have to wait until the caller hands it over to
                // us in order to drop it, though, so we pretend we're ready and mark ourselves to
                // drop the next item when `start_send` is called.
                //
                // One "gotcha" here is that the base sender is still trying to reserve a sending
                // slot for us, so technically it could complete between now and when we get to
                // `start_send` and actually drop the item.
                //
                // Based on the current behavior of `PollSender<T>`, the best thing we can do here
                // is to simply to drop the item and not abort the send, since that will leave
                // `PollSender<T>` armed for the next time we call `poll_reserve`.  Since buffers
                // are SPSC, there's no risk in trying up a sender slot.
                //
                // TODO: In the future, `PollSender<T>::start_send` may be tweaked to attempt a
                // call to `Sender<T>::try_send` as a last ditch effort when `PollSender<T>` has not
                // yet reserved the sending slot.  We could take advantage of this ourselves.
                WhenFull::DropNewest => (Poll::Ready(Ok(())), SendState::DropNext),
                // We're supposed to overflow.  Quickly check to make sure we even have an overflow
                // sender configured, and then figure out if the overflow sender can actually accept
                // a send at the moment.
                WhenFull::Overflow => match this.overflow.as_pin_mut() {
                    None => panic!("overflow mode set, but no overflow sender present"),
                    Some(overflow) => match overflow.poll_ready(cx) {
                        // Our overflow sender is ready for sending, so we mark ourselves so we know
                        // which sender to write to when `start_send` is called next.
                        Poll::Ready(result) => match result {
                            Ok(()) => (Poll::Ready(Ok(())), SendState::OverflowReady),
                            Err(e) => (Poll::Ready(Err(e)), SendState::Idle),
                        },
                        // Our overflow sender is not ready, either, so there's nothing else to do
                        // here except wait for a wakeup from either the base sender or overflow sender.
                        Poll::Pending => (Poll::Pending, SendState::Idle),
                    },
                },
            },
        };

        *this.state = next_state;
        result
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        let this = self.project();
        let item_size = this.instrumentation.as_ref().map(|_| item.size_of());

        let result = match this.state {
            // Sender isn't ready at all.
            SendState::Idle => panic!(
                "`start_send` should not be called unless `poll_ready` returned successfully"
            ),
            // We've been instructed to drop the next item.
            SendState::DropNext => {
                if let Some(instrumentation) = this.instrumentation.as_ref() {
                    instrumentation.try_increment_dropped_event_count(1);
                }
                Ok(())
            }
            // Base is ready, so send the item there.
            SendState::BaseReady => {
                let result = this.base.start_send(item);
                if result.is_ok() {
                    *this.base_flush = true;
                }
                result
            }
            // Overflow is ready, so send the item there.
            SendState::OverflowReady => {
                let result = this.overflow.as_pin_mut().unwrap().start_send(item);
                if result.is_ok() {
                    *this.overflow_flush = true;
                }
                result
            }
        };

        if let Some(item_size) = item_size {
            // Only update our instrumentation if _we_ got the item, not the overflow.
            let handle = this
                .instrumentation
                .as_ref()
                .expect("item_size can't be present without instrumentation");
            if *this.base_flush {
                handle.increment_received_event_count_and_byte_size(1, item_size as u64);
            }
        }

        *this.state = SendState::Idle;
        result
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.project();

        if *this.base_flush {
            ready!(this.base.poll_flush(cx))?;
            *this.base_flush = false;
        }

        if *this.overflow_flush {
            ready!(this.overflow.as_pin_mut().unwrap().poll_flush(cx))?;
            *this.overflow_flush = false;
        }

        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.project();

        if let Some(overflow) = this.overflow.as_pin_mut() {
            ready!(overflow.poll_close(cx))?;
        }

        ready!(this.base.poll_flush(cx))?;

        Poll::Ready(Ok(()))
    }
}
