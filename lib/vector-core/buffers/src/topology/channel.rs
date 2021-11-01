use std::{pin::Pin, task::{Context, Poll}};

use futures::{Sink, SinkExt, ready};
use tokio::sync::mpsc::error::SendError;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::PollSender;

use crate::WhenFull;

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
pub struct BufferSender<T> {
	base: PollSender<T>,
	base_ready: bool,
	drop_next: bool,
	overflow: Option<Pin<Box<BufferSender<T>>>>,
	overflow_ready: bool,
	when_full: WhenFull,
}

impl<T> BufferSender<T> {
	/// Creates a new [`BufferSender`] wrapping the given channel sender.
	pub(crate) fn new(sender: PollSender<T>, when_full: WhenFull) -> Self {
		Self {
			base: sender,
			base_ready: false,
			drop_next: false,
			overflow: None,
			overflow_ready: false,
			when_full,
		}
	}

	/// Creates a new [`BufferSender`] wrapping the given channel sender and overflow sender.
	pub(crate) fn with_overflow(sender: PollSender<T>, overflow: BufferSender<T>) -> Self {
		Self {
			base: sender,
			base_ready: false,
			drop_next: false,
			overflow: Some(Box::pin(overflow)),
			overflow_ready: false,
			when_full: WhenFull::Overflow,
		}
	}
}

impl<T> Sink<T> for BufferSender<T>
where
	T: Send + 'static,
{
    type Error = SendError<T>; 

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
		if self.base_ready || self.overflow_ready {
			// TODO: We can technically avoid panicking and just return Poll::Ready(Ok(())), based
			// on the documentation for `PollSender<T>`, but it would, practically speaking, be
			// considered invalid to poll readiness again if the previous call returned
			// successfully, given that `poll_ready` could, unbeknownst to the caller, be allocating
			// a resource of some sort each time.
			panic!("`poll_ready` should not be called again after a successful call");
		}

		match self.base.poll_send_done(cx) {
			// Our base sender is ready for sending, so we mark ourselves so we know which
			// sender to write to when `start_send` is called next.
			Poll::Ready(result) => match result {
				Ok(()) => {
					self.base_ready = true;
					Poll::Ready(Ok(()))
				},
				Err(e) => Poll::Ready(Err(e)),
			},
			// Our base sender is not ready, so figure out what we need to do based on the
			// configured "when full" mode.
			Poll::Pending => match self.when_full {
				// We need to block.  Nothing else to do, as the base sender will notify us when
				// there's capacity to do the send.
				WhenFull::Block => return Poll::Pending,
				// We need to drop the next item.  We have to wait until the caller hands it over to
				// us in order to drop it, though, so we pretend we're ready and mark ourselves to
				// drop the next item when `start_send` is called.
				WhenFull::DropNewest => {
					self.base_ready = true;
					self.drop_next = true;
					Poll::Ready(Ok(()))
				},
				// We're supposed to overflow.  Quickly check to make sure we even have an overflow
				// sender configured, and then figure out if the overflow sender can actually accept
				// a send at the moment.
				WhenFull::Overflow => match self.overflow.as_mut() {
					None => panic!("overflow mode set, but no overflow sender present"),
					Some(overflow) => match overflow.as_mut().poll_ready(cx) {
						// Our overflow sender is ready for sending, so we mark ourselves so we know
						// which sender to write to when `start_send` is called next.
						Poll::Ready(result) => match result {
							Ok(()) => {
								self.overflow_ready = true;
								Poll::Ready(Ok(()))
							},
							Err(e) => Poll::Ready(Err(e)),
						},
						// Our overflow sender is not ready, either, so there's nothing else to do
						// here except wait for a wakeup from either the base sender or overflow sender.
						Poll::Pending => Poll::Pending,
					}
				}
			}
		}
    }

    fn start_send(mut self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
		// TODO: we should probably use a state enum for base ready vs overflow ready vs drop next

		if !self.base_ready && !self.overflow_ready {
			// TODO: I don't super like panicking but this feels fine for the current design phase.
			panic!("`start_send` should not be called unless `poll_ready` returned successfully");
		}

		if self.base_ready {
			let result = if self.drop_next {
				// We've been instructed to drop the next item.
				//
				// TODO: need to emit a metric here that we dropped
				drop(item);
				self.drop_next = false;
				Ok(())
			} else {
				self.base.start_send(item)
			};
			self.base_ready = false;
			result
		} else {
			let result = match self.overflow.as_mut() {
				None => panic!("overflow mode set, but no overflow sender present"),
				Some(overflow) => overflow.as_mut().start_send(item),
			};
			self.overflow_ready = false;
			result
		}
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
		// TODO: do we actually want to track base vs overflow need for flush? right now, we're just
		// polling for flushes on both if possible, and sending the worst response back, but it's
		// kind of pointless to poll both if we haven't sent anything, although it _should_
		// technically be free/a no-op since it's all based on `PollSender::poll_send_done`.
        let base = self.base.poll_send_done(cx);
		let overflow = match self.overflow.as_mut() {
			None => Poll::Ready(Ok(())),
			Some(overflow) => overflow.as_mut().poll_flush(cx),
		};

		match (base, overflow) {
			// Any errors get bubbled up to the caller, other sender be damned.
			(Poll::Ready(Err(e)), _) => Poll::Ready(Err(e)),	
			(_, Poll::Ready(Err(e))) => Poll::Ready(Err(e)),
			// One of the senders is waiting to flush, so the caller needs to wait, too.
			(Poll::Pending, _) => Poll::Pending,
			(_, Poll::Pending) => Poll::Pending,
			// All good in the hood.
			(Poll::Ready(Ok(())), Poll::Ready(Ok(()))) => Poll::Ready(Ok(())),
		}
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
		let base = Pin::new(&mut self.base).poll_close(cx);
		let overflow = match self.overflow.as_mut() {
			None => Poll::Ready(Ok(())),
			Some(overflow) => overflow.as_mut().poll_close(cx),
		};

		match (base, overflow) {
			// Any errors get bubbled up to the caller, other sender be damned.
			(Poll::Ready(Err(e)), _) => Poll::Ready(Err(e)),	
			(_, Poll::Ready(Err(e))) => Poll::Ready(Err(e)),
			// One of the senders is waiting to close, so the caller needs to wait, too.
			(Poll::Pending, _) => Poll::Pending,
			(_, Poll::Pending) => Poll::Pending,
			// All good in the hood.
			(Poll::Ready(Ok(())), Poll::Ready(Ok(()))) => Poll::Ready(Ok(())),
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
pub struct BufferReceiver<T> {
	base: ReceiverStream<T>,
	overflow: Option<Pin<Box<BufferReceiver<T>>>>,
}

impl<T> BufferReceiver<T> {
	/// Creates a new [`BufferReceiver`] wrapping the given channel receiver.
	pub(crate) fn new(receiver: ReceiverStream<T>) -> Self {
		Self {
			base: receiver,
			overflow: None,
		}
	}

	/// Creates a new [`BufferReceiver`] wrapping the given channel receiver and overflow receiver.
	pub(crate) fn with_overflow(receiver: ReceiverStream<T>, overflow: BufferReceiver<T>) -> Self {
		Self {
			base: receiver,
			overflow: Some(Box::pin(overflow)),
		}
	}
}
