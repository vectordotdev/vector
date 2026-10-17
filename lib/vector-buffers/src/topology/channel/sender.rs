use std::{sync::Arc, time::Instant};

use async_recursion::async_recursion;
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
    /// Whether this backend can only persist items satisfying [`Bufferable::is_fully_encodable`].
    ///
    /// In-memory stages hold the in-memory representation and have no wire format, so they can
    /// accept any item regardless of its nesting depth. Disk stages encode to protobuf on write
    /// and cannot.
    ///
    /// Callers use this to avoid assuming a stage is constrained: an item that one stage cannot
    /// encode may be perfectly storable by another, so the check must be asked of the specific
    /// stage rather than applied to every topology.
    pub(crate) fn requires_encodable_items(&self) -> bool {
        match self {
            Self::InMemory(_) => false,
            Self::DiskV2(_) => true,
        }
    }

    pub(crate) async fn send(&mut self, item: T) -> crate::Result<TryWriteOutcome<T>> {
        match self {
            Self::InMemory(tx) => tx
                .send(item)
                .await
                .map(|()| TryWriteOutcome::Written)
                .map_err(Into::into),
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
                    return Ok(TryWriteOutcome::Dropped);
                };
                let item_size = track_filter_drops(&writer, &item, pre_count, pre_size);

                let max_record_size = writer.max_record_size();
                if item_size <= max_record_size {
                    return writer
                        .write_record_outcome(item)
                        .await
                        .map_err(unrecoverable_writer_error);
                }

                // Every piece is written, waiting for room as needed, so the item counts as written
                // if any part of it made it to disk. Pieces the writer drops are already accounted
                // for in its ledger.
                let mut outcome = TryWriteOutcome::Dropped;
                for piece in split_to_fit(item, item_size, max_record_size) {
                    if let TryWriteOutcome::Written = writer
                        .write_record_outcome(piece)
                        .await
                        .map_err(unrecoverable_writer_error)?
                    {
                        outcome = TryWriteOutcome::Written;
                    }
                }
                Ok(outcome)
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

                // Filtering here is unconditional and independent of current occupancy.
                // Whether an unencodable item should be dropped or handed to an overflow
                // stage is a `WhenFull` policy decision, so it is made in `BufferSender`
                // before the item ever reaches this backend: `WhenFull::Overflow` diverts
                // items failing `is_fully_encodable` straight to the overflow stage, and
                // anything arriving here is therefore expected to be persistable. Keeping
                // the filter unconditional means a given item is treated the same at 99%
                // full as at 100% full.
                let pre_count = item.event_count() as u64;
                let pre_size = item.size_of() as u64;
                let Some(item) = item.filter_unencodable() else {
                    writer.track_dropped(pre_count, pre_size);
                    return Ok(TryWriteOutcome::Dropped);
                };
                let item_size = track_filter_drops(&writer, &item, pre_count, pre_size);

                let max_record_size = writer.max_record_size();
                if item_size <= max_record_size {
                    return writer
                        .try_write_record(item)
                        .await
                        .map_err(unrecoverable_writer_error);
                }

                // Pieces are written in order until one does not fit. Everything from that piece
                // on is rejoined and handed back as `Full`, so the caller drops or overflows only
                // what is not on disk, and in the original order.
                let mut outcome = TryWriteOutcome::Dropped;
                let mut pieces = split_to_fit(item, item_size, max_record_size).into_iter();
                while let Some(piece) = pieces.next() {
                    match writer
                        .try_write_record(piece)
                        .await
                        .map_err(unrecoverable_writer_error)?
                    {
                        TryWriteOutcome::Written => outcome = TryWriteOutcome::Written,
                        TryWriteOutcome::Dropped => {}
                        TryWriteOutcome::Full(piece) => {
                            return Ok(TryWriteOutcome::Full(pieces.fold(piece, T::rejoin)));
                        }
                    }
                }
                Ok(outcome)
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

/// Reports the sub-items `filter_unencodable` removed from an item that held `pre_count` events
/// of `pre_size` bytes, and returns the size of what is left.
fn track_filter_drops<T>(
    writer: &disk_v2::BufferWriter<T, ProductionFilesystem>,
    item: &T,
    pre_count: u64,
    pre_size: u64,
) -> usize
where
    T: Bufferable,
{
    if item.event_count() as u64 == pre_count {
        // `pre_size` came from a `usize`; if it somehow does not fit, treating the item as huge
        // only sends it down the split path, which measures the pieces again.
        return usize::try_from(pre_size).unwrap_or(usize::MAX);
    }
    let item_size = item.size_of();
    let dropped_events = pre_count - item.event_count() as u64;
    let dropped_bytes = pre_size.saturating_sub(item_size as u64);
    writer.track_dropped(dropped_events, dropped_bytes);
    item_size
}

/// Splits `item`, whose in-memory size is `item_size`, into pieces of at most `max_record_size`
/// bytes each, in their original order.
///
/// The disk buffer writes an item as a single record and drops the whole record when it encodes
/// to more than `max_record_size`, taking every event in it along. Splitting first means only an
/// event that is too large on its own is lost. The in-memory size stands in for the encoded size,
/// which is not known until the item is consumed by encoding. It is normally the larger of the two
/// for events; a piece that still encodes over the limit is dropped by the writer, as any oversized
/// record was before. Only oversized items pay for the extra size walks.
fn split_to_fit<T>(item: T, item_size: usize, max_record_size: usize) -> Vec<T>
where
    T: Bufferable,
{
    fn split_into<T: Bufferable>(item: T, item_size: usize, max: usize, pieces: &mut Vec<T>) {
        if item_size <= max {
            pieces.push(item);
            return;
        }
        match item.split_in_half() {
            Ok((front, back)) => {
                let (front_size, back_size) = (front.size_of(), back.size_of());
                split_into(front, front_size, max, pieces);
                split_into(back, back_size, max, pieces);
            }
            // A single sub-item over the limit; the writer drops it as unwritable.
            Err(item) => pieces.push(item),
        }
    }

    let mut pieces = Vec::new();
    split_into(item, item_size, max_record_size, &mut pieces);
    if pieces.len() > 1 {
        debug!(
            message =
                "Split an item over the maximum record size into several disk buffer records.",
            item_size,
            max_record_size,
            records = pieces.len(),
        );
    }
    pieces
}

/// Record-level failures that can never succeed (a record too large to encode within the max
/// record size) are handled inside the writer and surfaced as a dropped outcome. Anything that
/// reaches this point -- I/O errors, serialization failures, an inconsistent writer state -- is
/// genuinely unrecoverable.
fn unrecoverable_writer_error<T: Bufferable>(e: disk_v2::WriterError<T>) -> vector_common::Error {
    error!("Disk buffer writer has encountered an unrecoverable error.");

    e.into()
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
#[derive(Clone, derive_more::Debug)]
pub struct BufferSender<T: Bufferable> {
    base: SenderAdapter<T>,
    // `overflow` is self-referential, so formatting it via a plain placeholder would make
    // derive_more infer `Option<Box<BufferSender<T>>>: Debug`, overflowing (E0275). A method-call
    // expression skips bound inference while formatting the same value.
    #[debug("{:?}", overflow.as_ref())]
    overflow: Option<Box<BufferSender<T>>>,
    when_full: WhenFull,
    usage_instrumentation: Option<BufferUsageHandle>,
    #[debug(skip)]
    send_duration: Option<Registered<BufferSendDuration>>,
    #[debug(skip)]
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
            WhenFull::Overflow => {
                // An item the base stage can never encode is routed to the overflow stage
                // intact, whatever the current occupancy. Deciding this here, rather than
                // letting the backend filter it, is what makes the behaviour
                // state-independent: previously an over-nested item was pruned while the
                // disk had room and forwarded whole once the disk reported full, so the
                // same item took different paths at 99% and 100%.
                //
                // The check is gated on the base stage actually having a wire-format
                // constraint. A memory stage overflowing to disk can store an over-nested
                // item perfectly well, so diverting it past memory would send an item the
                // base could have kept to a stage that must drop it.
                if self.base.requires_encodable_items() && !item.is_fully_encodable() {
                    self.overflow
                        .as_mut()
                        .unwrap_or_else(|| unreachable!("overflow must exist"))
                        .send(item, send_reference)
                        .await?;
                    UsageAccounting::NotAccepted
                } else {
                    match self.base.try_send(item).await? {
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
                    }
                }
            }
        };

        // Backend filter drops are accounted directly through the backend's own
        // usage handle (e.g. disk-v2's ledger), so they show up in the buffer
        // stage's `received` / `dropped` metrics even when the `BufferSender`
        // does not carry instrumentation. This block only reports fullness-driven
        // drops captured via `was_dropped`.
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
