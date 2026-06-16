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

/// Events/bytes dropped by `Bufferable::filter_unencodable` inside the backend
/// dispatch. Only the disk-v2 backend invokes the filter (it is the only one with
/// wire-format constraints); in-memory backends always return the default zero
/// value.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FilterDrops {
    pub events: u64,
    pub bytes: u64,
}

/// Outcome of [`SenderAdapter::try_send`]. Carries both whatever the filter dropped
/// (which the caller still needs to account for in buffer instrumentation) and the
/// item that did not fit, if any (which the caller may forward to an overflow stage).
pub(crate) struct TrySendOutcome<T: Bufferable> {
    pub filter_drops: FilterDrops,
    pub rejected: Option<T>,
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
    pub(crate) async fn send(&mut self, item: T) -> crate::Result<FilterDrops> {
        match self {
            Self::InMemory(tx) => tx
                .send(item)
                .await
                .map(|()| FilterDrops::default())
                .map_err(Into::into),
            Self::DiskV2(writer) => {
                let pre_count = item.event_count() as u64;
                let pre_size = item.size_of() as u64;
                let Some(item) = item.filter_unencodable() else {
                    return Ok(FilterDrops {
                        events: pre_count,
                        bytes: pre_size,
                    });
                };
                let drops = FilterDrops {
                    events: pre_count - item.event_count() as u64,
                    bytes: pre_size.saturating_sub(item.size_of() as u64),
                };

                let mut writer = writer.lock().await;

                writer.write_record(item).await.map(|_| drops).map_err(|e| {
                    error!("Disk buffer writer has encountered an unrecoverable error.");

                    e.into()
                })
            }
        }
    }

    pub(crate) async fn try_send(&mut self, item: T) -> crate::Result<TrySendOutcome<T>> {
        match self {
            Self::InMemory(tx) => Ok(TrySendOutcome {
                filter_drops: FilterDrops::default(),
                rejected: tx
                    .try_send(item)
                    .err()
                    .map(super::limited_queue::TrySendError::into_inner),
            }),
            Self::DiskV2(writer) => {
                let pre_count = item.event_count() as u64;
                let pre_size = item.size_of() as u64;
                let Some(item) = item.filter_unencodable() else {
                    return Ok(TrySendOutcome {
                        filter_drops: FilterDrops {
                            events: pre_count,
                            bytes: pre_size,
                        },
                        rejected: None,
                    });
                };
                let filter_drops = FilterDrops {
                    events: pre_count - item.event_count() as u64,
                    bytes: pre_size.saturating_sub(item.size_of() as u64),
                };

                let mut writer = writer.lock().await;

                writer
                    .try_write_record(item)
                    .await
                    .map(|rejected| TrySendOutcome {
                        filter_drops,
                        rejected,
                    })
                    .map_err(|e| {
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

        let mut rejected_sizing: Option<(usize, usize)> = None;
        let filter_drops;

        if let Some(instrumentation) = self.usage_instrumentation.as_ref()
            && let Some((item_count, item_size)) = item_sizing
        {
            instrumentation
                .increment_received_event_count_and_byte_size(item_count as u64, item_size as u64);
        }
        match self.when_full {
            WhenFull::Block => filter_drops = self.base.send(item).await?,
            WhenFull::DropNewest => {
                let outcome = self.base.try_send(item).await?;
                filter_drops = outcome.filter_drops;
                if let Some(rejected) = outcome.rejected {
                    rejected_sizing = Some((rejected.event_count(), rejected.size_of()));
                }
            }
            WhenFull::Overflow => {
                let outcome = self.base.try_send(item).await?;
                filter_drops = outcome.filter_drops;
                if let Some(rejected) = outcome.rejected {
                    rejected_sizing = Some((rejected.event_count(), rejected.size_of()));
                    self.overflow
                        .as_mut()
                        .unwrap_or_else(|| unreachable!("overflow must exist"))
                        .send(rejected, send_reference)
                        .await?;
                }
            }
        }

        if let Some(instrumentation) = self.usage_instrumentation.as_ref() {
            // Backend-filtered sub-items never reach the buffer; report them as an
            // unintentional buffer drop so `buffer_size_*` (received - left) stays
            // consistent with what is actually queued, rather than staying inflated
            // by the filtered count forever.
            if filter_drops.events > 0 || filter_drops.bytes > 0 {
                instrumentation.increment_dropped_event_count_and_byte_size(
                    filter_drops.events,
                    filter_drops.bytes,
                    false,
                );
            }
            // Fullness-driven drops use the post-filter sizing of the rejected item,
            // not the pre-filter sizing captured above — the filter portion has
            // already been accounted for as an unintentional drop.
            if let Some((rejected_count, rejected_size)) = rejected_sizing {
                instrumentation.increment_dropped_event_count_and_byte_size(
                    rejected_count as u64,
                    rejected_size as u64,
                    true,
                );
            }
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
