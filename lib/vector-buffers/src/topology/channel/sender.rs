// Derivative's Debug impl generates 'let _ = field.fmt(f)' which triggers this lint.
#![allow(clippy::let_underscore_must_use)]

use std::{sync::Arc, time::Instant};

use async_recursion::async_recursion;
use derivative::Derivative;
use tokio::sync::Mutex;
use tracing::Span;
use vector_common::internal_event::{InternalEventHandle, Registered, register};

use super::limited_queue::LimitedSender;
use crate::{
    BufferInstrumentation, Bufferable, WhenFull,
    buffer_usage_data::BufferUsageHandle,
    internal_events::BufferSendDuration,
    variants::disk_v2::{self, ProductionFilesystem, TryWriteOutcome},
};

/// Adapter for papering over various sender backends.
#[derive(Clone, Debug)]
pub enum SenderAdapter<T: Bufferable> {
    /// The in-memory channel buffer.
    InMemory(LimitedSender<T>),

    /// The disk v2 buffer.
    DiskV2(Arc<Mutex<disk_v2::BufferWriter<T, ProductionFilesystem>>>),
}

impl<T: Bufferable> From<LimitedSender<T>> for SenderAdapter<T> {
    fn from(v: LimitedSender<T>) -> Self {
        Self::InMemory(v)
    }
}

impl<T: Bufferable> From<disk_v2::BufferWriter<T, ProductionFilesystem>> for SenderAdapter<T> {
    fn from(v: disk_v2::BufferWriter<T, ProductionFilesystem>) -> Self {
        Self::DiskV2(Arc::new(Mutex::new(v)))
    }
}

impl<T> SenderAdapter<T>
where
    T: Bufferable,
{
    pub(crate) async fn send(&mut self, item: T) -> crate::Result<TryWriteOutcome<T>> {
        match self {
            Self::InMemory(tx) => tx
                .send(item)
                .await
                .map(|()| TryWriteOutcome::Written)
                .map_err(Into::into),
            Self::DiskV2(writer) => {
                let mut writer = writer.lock().await;

                writer.write_record_outcome(item).await.map_err(|e| {
                    // Record-level failures that can never succeed (a record too large to encode
                    // within the max record size) are handled inside the writer and surfaced as a
                    // dropped outcome. Anything that reaches this point -- I/O errors,
                    // serialization failures, an inconsistent writer state -- is genuinely
                    // unrecoverable.
                    error!("Disk buffer writer has encountered an unrecoverable error.");

                    e.into()
                })
            }
        }
    }

    pub(crate) async fn try_send(&mut self, item: T) -> crate::Result<TryWriteOutcome<T>> {
        match self {
            Self::InMemory(tx) => tx
                .try_send(item)
                .map(|()| TryWriteOutcome::Written)
                .or_else(|e| Ok(TryWriteOutcome::Full(e.into_inner()))),
            Self::DiskV2(writer) => {
                let mut writer = writer.lock().await;

                writer.try_write_record(item).await.map_err(|e| {
                    // Record-level failures that can never succeed (a record too large to encode
                    // within the max record size) are handled inside the writer and surfaced as a
                    // dropped outcome. Anything that reaches this point -- I/O errors,
                    // serialization failures, an inconsistent writer state -- is genuinely
                    // unrecoverable.
                    error!("Disk buffer writer has encountered an unrecoverable error.");

                    e.into()
                })
            }
        }
    }

    pub(crate) async fn flush(&mut self) -> crate::Result<()> {
        match self {
            Self::InMemory(_) => Ok(()),
            Self::DiskV2(writer) => {
                let mut writer = writer.lock().await;
                writer.flush().await.map_err(|e| {
                    // Errors on the I/O path, which is all that flushing touches, are never recoverable.
                    error!("Disk buffer writer has encountered an unrecoverable error.");

                    e.into()
                })
            }
        }
    }

    pub fn capacity(&self) -> Option<usize> {
        match self {
            Self::InMemory(tx) => Some(tx.available_capacity()),
            Self::DiskV2(_) => None,
        }
    }
}

enum UsageAccounting {
    Accepted,
    DroppedNewest,
    NotAccepted,
}

impl UsageAccounting {
    fn record(self, instrumentation: &BufferUsageHandle, item_count: usize, item_size: usize) {
        match self {
            Self::Accepted => instrumentation
                .increment_received_event_count_and_byte_size(item_count as u64, item_size as u64),
            Self::DroppedNewest => {
                instrumentation.increment_received_event_count_and_byte_size(
                    item_count as u64,
                    item_size as u64,
                );
                instrumentation.increment_dropped_event_count_and_byte_size(
                    item_count as u64,
                    item_size as u64,
                    true,
                );
            }
            Self::NotAccepted => {}
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
///
/// TODO: We should eventually rework `BufferSender`/`BufferReceiver` so that they contain a vector
/// of the fields we already have here, but instead of cascading via calling into `overflow`, we'd
/// linearize the nesting instead, so that `BufferSender` would only ever be calling the underlying
/// `SenderAdapter` instances instead... which would let us get rid of the boxing and
/// `#[async_recursion]` stuff.
#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct BufferSender<T: Bufferable> {
    base: SenderAdapter<T>,
    overflow: Option<Box<BufferSender<T>>>,
    when_full: WhenFull,
    usage_instrumentation: Option<BufferUsageHandle>,
    #[derivative(Debug = "ignore")]
    send_duration: Option<Registered<BufferSendDuration>>,
    #[derivative(Debug = "ignore")]
    custom_instrumentation: Option<Arc<dyn BufferInstrumentation<T>>>,
}

impl<T: Bufferable> BufferSender<T> {
    /// Creates a new [`BufferSender`] wrapping the given channel sender.
    pub fn new(base: SenderAdapter<T>, when_full: WhenFull) -> Self {
        Self {
            base,
            overflow: None,
            when_full,
            usage_instrumentation: None,
            send_duration: None,
            custom_instrumentation: None,
        }
    }

    /// Creates a new [`BufferSender`] wrapping the given channel sender and overflow sender.
    pub fn with_overflow(base: SenderAdapter<T>, overflow: BufferSender<T>) -> Self {
        Self {
            base,
            overflow: Some(Box::new(overflow)),
            when_full: WhenFull::Overflow,
            usage_instrumentation: None,
            send_duration: None,
            custom_instrumentation: None,
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
    }

    /// Configures this sender to instrument the items passing through it.
    pub fn with_usage_instrumentation(&mut self, handle: BufferUsageHandle) {
        self.usage_instrumentation = Some(handle);
    }

    /// Configures this sender to instrument the send duration.
    pub fn with_send_duration_instrumentation(&mut self, stage: usize, span: &Span) {
        let _enter = span.enter();
        self.send_duration = Some(register(BufferSendDuration { stage }));
    }

    /// Configures this sender to invoke a custom instrumentation hook.
    pub fn with_custom_instrumentation(&mut self, instrumentation: impl BufferInstrumentation<T>) {
        self.custom_instrumentation = Some(Arc::new(instrumentation));
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

    #[async_recursion]
    pub async fn send(
        &mut self,
        mut item: T,
        send_reference: Option<Instant>,
    ) -> crate::Result<()> {
        if let Some(instrumentation) = self.custom_instrumentation.as_ref() {
            instrumentation.on_send(&mut item);
        }
        let item_sizing = self
            .usage_instrumentation
            .as_ref()
            .map(|_| (item.event_count(), item.size_of()));

        let accounting = match self.when_full {
            WhenFull::Block => match self.base.send(item).await? {
                TryWriteOutcome::Written => UsageAccounting::Accepted,
                TryWriteOutcome::Full(_) => unreachable!("blocking sends wait until space exists"),
                TryWriteOutcome::Dropped => UsageAccounting::NotAccepted,
            },
            WhenFull::DropNewest => match self.base.try_send(item).await? {
                TryWriteOutcome::Written => UsageAccounting::Accepted,
                TryWriteOutcome::Full(_) => UsageAccounting::DroppedNewest,
                TryWriteOutcome::Dropped => UsageAccounting::NotAccepted,
            },
            WhenFull::Overflow => match self.base.try_send(item).await? {
                TryWriteOutcome::Written => UsageAccounting::Accepted,
                TryWriteOutcome::Full(item) => {
                    self.overflow
                        .as_mut()
                        .unwrap_or_else(|| unreachable!("overflow must exist"))
                        .send(item, send_reference)
                        .await?;
                    UsageAccounting::NotAccepted
                }
                TryWriteOutcome::Dropped => UsageAccounting::NotAccepted,
            },
        };

        if let Some(instrumentation) = self.usage_instrumentation.as_ref()
            && let Some((item_count, item_size)) = item_sizing
        {
            accounting.record(instrumentation, item_count, item_size);
        }
        if let Some(send_duration) = self.send_duration.as_ref()
            && let Some(send_reference) = send_reference
        {
            send_duration.emit(send_reference.elapsed());
        }

        Ok(())
    }

    #[async_recursion]
    pub async fn flush(&mut self) -> crate::Result<()> {
        self.base.flush().await?;
        if let Some(overflow) = self.overflow.as_mut() {
            overflow.flush().await?;
        }

        Ok(())
    }
}
