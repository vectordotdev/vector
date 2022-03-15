use std::pin::Pin;

use async_recursion::async_recursion;
use tokio::select;

use super::limited_queue::LimitedReceiver;
use crate::{
    buffer_usage_data::BufferUsageHandle,
    variants::{disk_v1, disk_v2},
    Bufferable,
};

/// Adapter for papering over various receiver backends.
#[derive(Debug)]
pub enum ReceiverAdapter<T: Bufferable> {
    /// The in-memory channel buffer.
    InMemory(LimitedReceiver<T>),

    /// The disk v1 buffer.
    DiskV1(disk_v1::Reader<T>),

    /// The disk v2 buffer.
    DiskV2(disk_v2::Reader<T>),
}

impl<T: Bufferable> From<LimitedReceiver<T>> for ReceiverAdapter<T> {
    fn from(v: LimitedReceiver<T>) -> Self {
        Self::InMemory(v)
    }
}

impl<T: Bufferable> From<disk_v1::Reader<T>> for ReceiverAdapter<T> {
    fn from(v: disk_v1::Reader<T>) -> Self {
        Self::DiskV1(v)
    }
}

impl<T: Bufferable> From<disk_v2::Reader<T>> for ReceiverAdapter<T> {
    fn from(v: disk_v2::Reader<T>) -> Self {
        Self::DiskV2(v)
    }
}

impl<T> ReceiverAdapter<T>
where
    T: Bufferable,
{
    pub async fn next(&mut self) -> Option<T> {
        match self {
            ReceiverAdapter::InMemory(rx) => rx.next().await,
            ReceiverAdapter::DiskV1(reader) => reader.next().await,
            ReceiverAdapter::DiskV2(reader) => reader
                .next()
                .await
                .expect("reader encountered unrecoverable error"),
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
#[derive(Debug)]
pub struct BufferReceiver<T: Bufferable> {
    base: ReceiverAdapter<T>,
    overflow: Option<Box<BufferReceiver<T>>>,
    instrumentation: Option<BufferUsageHandle>,
}

impl<T: Bufferable> BufferReceiver<T> {
    /// Creates a new [`BufferReceiver`] wrapping the given channel receiver.
    pub fn new(base: ReceiverAdapter<T>) -> Self {
        Self {
            base,
            overflow: None,
            instrumentation: None,
        }
    }

    /// Creates a new [`BufferReceiver`] wrapping the given channel receiver and overflow receiver.
    pub fn with_overflow(base: ReceiverAdapter<T>, overflow: BufferReceiver<T>) -> Self {
        Self {
            base,
            overflow: Some(Box::new(overflow)),
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

    #[async_recursion]
    pub async fn next(&mut self) -> Option<T> {
        // We want to poll both our base and overflow receivers without waiting for one or the
        // other to entirely drain before checking the other.  This ensures that we're fairly
        // servicing both receivers, and avoiding stalls in one or the other.
        //
        // This is primarily important in situations where an overflow-triggering event has
        // occurred, and is over, and items are flowing through the base receiver.  If we waited to
        // entirely drain the overflow receiver, we might cause another small stall of the pipeline
        // attached to the base receiver.
        let overflow = self.overflow.as_mut().map(Pin::new);

        let (item, from_base) = match overflow {
            None => match self.base.next().await {
                Some(item) => (item, true),
                None => return None,
            },
            Some(mut overflow) => {
                select! {
                    Some(item) = overflow.next() => (item, false),
                    Some(item) = self.base.next() => (item, true),
                    else => return None,
                }
            }
        };

        // If instrumentation is enabled, and we got the item from the base receiver, then and only
        // then do we track sending the event out.
        if let Some(handle) = self.instrumentation.as_ref() {
            if from_base {
                handle.increment_sent_event_count_and_byte_size(
                    item.event_count() as u64,
                    item.size_of() as u64,
                );
            }
        }

        Some(item)
    }
}
