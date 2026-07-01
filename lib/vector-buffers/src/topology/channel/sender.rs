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
    variants::disk_v2::{self, ProductionFilesystem},
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
    pub(crate) async fn send(&mut self, item: T) -> crate::Result<()> {
        match self {
            Self::InMemory(tx) => tx.send(item).await.map_err(Into::into),
            Self::DiskV2(writer) => {
                let pre_count = item.event_count() as u64;
                let pre_size = item.size_of() as u64;
                let mut writer = writer.lock().await;

                let Some(item) = item.filter_unencodable() else {
                    // The whole item was filtered out (e.g. every sub-item over the
                    // protobuf nesting budget). Report the drop directly via the
                    // ledger's usage handle so it shows up in the disk-v2 stage's
                    // `received` / `dropped` metrics — `BufferSender` does not carry
                    // its own handle for backends that `provides_instrumentation()`.
                    writer.track_dropped(pre_count, pre_size);
                    return Ok(());
                };
                if item.event_count() as u64 != pre_count {
                    let dropped_events = pre_count - item.event_count() as u64;
                    let dropped_bytes = pre_size.saturating_sub(item.size_of() as u64);
                    writer.track_dropped(dropped_events, dropped_bytes);
                }

                writer.write_record(item).await.map(|_| ()).map_err(|e| {
                    error!("Disk buffer writer has encountered an unrecoverable error.");

                    e.into()
                })
            }
        }
    }

    pub(crate) async fn try_send(&mut self, item: T) -> crate::Result<Option<T>> {
        match self {
            Self::InMemory(tx) => Ok(tx
                .try_send(item)
                .err()
                .map(super::limited_queue::TrySendError::into_inner)),
            Self::DiskV2(writer) => {
                let mut writer = writer.lock().await;

                // If the disk buffer is already at its size limit, hand the item off
                // to the caller unfiltered. The caller forwards it to the overflow
                // stage in `WhenFull::Overflow` mode, and the overflow stage may be
                // an in-memory buffer with no wire-format constraint — filtering
                // here would needlessly drop sub-items that the overflow could
                // accept. Holding the writer lock makes the check race-free against
                // other writers (only writers grow the buffer; readers only shrink).
                if writer.is_buffer_full() {
                    return Ok(Some(item));
                }

                // KNOWN LIMITATION (accepted; tracked as a follow-up): past the
                // steady-state-full check above, over-budget sub-items are filtered
                // and dropped here even in `WhenFull::Overflow`, so a non-protobuf
                // overflow stage (e.g. in-memory) never gets the chance to accept
                // them. This surfaces two ways:
                //   1. the item is partially over-budget and `try_write_record`
                //      below then rejects the *remainder* for fullness — the
                //      overflow receives the item minus the already-dropped events;
                //   2. the item is fully over-budget — `filter_unencodable` returns
                //      `None` and the whole item is dropped before any capacity
                //      check, so nothing overflows.
                // Routing unencodable items by `WhenFull` (drop in Block/DropNewest,
                // overflow otherwise) is a `BufferSender`-level policy decision,
                // whereas filtering lives here in the backend; reconciling the two
                // is deferred. The window is narrow and atypical: it requires a
                // disk-v2 stage in `Overflow` mode (disk is normally the terminal
                // Block stage), a non-protobuf overflow target, an over-budget
                // event (>32 nesting levels), and a downstream egress that could
                // actually deliver it. In other topologies these events are dropped
                // a stage later regardless.
                let pre_count = item.event_count() as u64;
                let pre_size = item.size_of() as u64;
                let Some(item) = item.filter_unencodable() else {
                    writer.track_dropped(pre_count, pre_size);
                    return Ok(None);
                };
                if item.event_count() as u64 != pre_count {
                    let dropped_events = pre_count - item.event_count() as u64;
                    let dropped_bytes = pre_size.saturating_sub(item.size_of() as u64);
                    writer.track_dropped(dropped_events, dropped_bytes);
                }

                writer.try_write_record(item).await.map_err(|e| {
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

        let mut was_dropped = false;

        if let Some(instrumentation) = self.usage_instrumentation.as_ref()
            && let Some((item_count, item_size)) = item_sizing
        {
            instrumentation
                .increment_received_event_count_and_byte_size(item_count as u64, item_size as u64);
        }
        match self.when_full {
            WhenFull::Block => self.base.send(item).await?,
            WhenFull::DropNewest => {
                if self.base.try_send(item).await?.is_some() {
                    was_dropped = true;
                }
            }
            WhenFull::Overflow => {
                if let Some(item) = self.base.try_send(item).await? {
                    was_dropped = true;
                    self.overflow
                        .as_mut()
                        .unwrap_or_else(|| unreachable!("overflow must exist"))
                        .send(item, send_reference)
                        .await?;
                }
            }
        }

        // Backend filter drops are accounted directly through the backend's own
        // usage handle (e.g. disk-v2's ledger), so they show up in the buffer
        // stage's `received` / `dropped` metrics even when the `BufferSender`
        // does not carry instrumentation. This block only reports fullness-driven
        // drops captured via `was_dropped`.
        if let Some(instrumentation) = self.usage_instrumentation.as_ref()
            && let Some((item_count, item_size)) = item_sizing
            && was_dropped
        {
            instrumentation.increment_dropped_event_count_and_byte_size(
                item_count as u64,
                item_size as u64,
                true,
            );
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
