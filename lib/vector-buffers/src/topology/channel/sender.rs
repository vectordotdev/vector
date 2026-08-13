// Derivative's Debug impl generates 'let _ = field.fmt(f)' which triggers this lint.
#![allow(clippy::let_underscore_must_use)]

use std::{fmt, io, sync::Arc, time::Instant};

use async_recursion::async_recursion;
use derivative::Derivative;
use tokio::sync::{mpsc, oneshot, watch};
use tracing::Span;
use vector_common::internal_event::{InternalEventHandle, Registered, register};

use super::limited_queue::LimitedSender;
use crate::{
    BufferInstrumentation, Bufferable, WhenFull,
    buffer_usage_data::BufferUsageHandle,
    internal_events::BufferSendDuration,
    variants::disk_v2::{self, ProductionFilesystem, TryWriteOutcome},
};

const DISK_V2_WRITER_QUEUE_CAPACITY: usize = 1;

enum DiskV2WriterCommand<T: Bufferable> {
    Write {
        item: T,
        blocking: bool,
        response: oneshot::Sender<Result<TryWriteOutcome<T>, disk_v2::WriterError<T>>>,
    },
    Flush {
        response: oneshot::Sender<io::Result<()>>,
    },
}

#[derive(Clone)]
pub struct DiskV2Sender<T: Bufferable> {
    commands: mpsc::Sender<DiskV2WriterCommand<T>>,
    _shutdown: watch::Sender<()>,
}

impl<T: Bufferable> DiskV2Sender<T> {
    fn new(mut writer: disk_v2::BufferWriter<T, ProductionFilesystem>) -> Self {
        let (commands, mut command_rx) = mpsc::channel(DISK_V2_WRITER_QUEUE_CAPACITY);
        let (shutdown, shutdown_rx) = watch::channel(());
        writer.set_shutdown(shutdown_rx);
        vector_common::spawn_in_current_span(async move {
            while let Some(command) = command_rx.recv().await {
                if Self::process_command(&mut writer, command).await {
                    break;
                }
            }
        });

        Self {
            commands,
            _shutdown: shutdown,
        }
    }

    // The task is the cancellation boundary for disk I/O. While the topology is active, once
    // the queue accepts a command, this owner finishes the write and its visibility flush even
    // if the requesting future is dropped. When the last sender is dropped, only waits for
    // reader progress are interrupted; any file I/O already in progress still finishes.
    async fn process_command(
        writer: &mut disk_v2::BufferWriter<T, ProductionFilesystem>,
        command: DiskV2WriterCommand<T>,
    ) -> bool {
        match command {
            DiskV2WriterCommand::Write {
                item,
                blocking,
                response,
            } => {
                let result = if blocking {
                    writer.write_record_outcome(item).await
                } else {
                    writer.try_write_record(item).await
                };
                let result = match result {
                    Ok(outcome) => writer
                        .flush()
                        .await
                        .map(|()| outcome)
                        .map_err(|source| disk_v2::WriterError::Io { source }),
                    other => other,
                };
                let shutting_down = matches!(&result, Err(disk_v2::WriterError::Shutdown));
                let failed = result.is_err() && !shutting_down;
                if failed {
                    writer.fail();
                }
                let _ = response.send(result);
                failed || shutting_down
            }
            DiskV2WriterCommand::Flush { response } => {
                let result = writer.flush().await;
                let failed = result.is_err();
                if failed {
                    writer.fail();
                }
                let _ = response.send(result);
                failed
            }
        }
    }

    async fn write(&self, item: T, blocking: bool) -> crate::Result<TryWriteOutcome<T>> {
        let (response, response_rx) = oneshot::channel();
        self.commands
            .send(DiskV2WriterCommand::Write {
                item,
                blocking,
                response,
            })
            .await
            .map_err(|_| io::Error::other("disk buffer writer task stopped"))?;

        response_rx
            .await
            .map_err(|_| io::Error::other("disk buffer writer task stopped"))?
            .map_err(|error| {
                error!(%error, "Disk buffer writer encountered an unrecoverable error.");
                error.into()
            })
    }

    async fn flush(&self) -> crate::Result<()> {
        let (response, response_rx) = oneshot::channel();
        self.commands
            .send(DiskV2WriterCommand::Flush { response })
            .await
            .map_err(|_| io::Error::other("disk buffer writer task stopped"))?;

        response_rx
            .await
            .map_err(|_| io::Error::other("disk buffer writer task stopped"))?
            .map_err(|error| {
                error!(%error, "Disk buffer writer encountered an unrecoverable error.");
                error.into()
            })
    }
}

impl<T: Bufferable> fmt::Debug for DiskV2Sender<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("DiskV2Sender").finish()
    }
}

/// Adapter for papering over various sender backends.
#[derive(Clone, Debug)]
pub enum SenderAdapter<T: Bufferable> {
    /// The in-memory channel buffer.
    InMemory(LimitedSender<T>),

    /// The disk v2 buffer.
    DiskV2(DiskV2Sender<T>),
}

impl<T: Bufferable> From<LimitedSender<T>> for SenderAdapter<T> {
    fn from(v: LimitedSender<T>) -> Self {
        Self::InMemory(v)
    }
}

impl<T: Bufferable> From<disk_v2::BufferWriter<T, ProductionFilesystem>> for SenderAdapter<T> {
    fn from(v: disk_v2::BufferWriter<T, ProductionFilesystem>) -> Self {
        Self::DiskV2(DiskV2Sender::new(v))
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
            Self::DiskV2(writer) => writer.write(item, true).await,
        }
    }

    pub(crate) async fn try_send(&mut self, item: T) -> crate::Result<TryWriteOutcome<T>> {
        match self {
            Self::InMemory(tx) => tx
                .try_send(item)
                .map(|()| TryWriteOutcome::Written)
                .or_else(|e| Ok(TryWriteOutcome::Full(e.into_inner()))),
            Self::DiskV2(writer) => writer.write(item, false).await,
        }
    }

    pub(crate) async fn flush(&mut self) -> crate::Result<()> {
        match self {
            Self::InMemory(_) => Ok(()),
            Self::DiskV2(writer) => writer.flush().await,
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{DiskV2Sender, DiskV2WriterCommand, TryWriteOutcome, oneshot};
    use crate::{
        buffer_usage_data::BufferUsageHandle,
        test::{SizedRecord, with_temp_dir},
        variants::disk_v2::{Buffer, DiskBufferConfigBuilder},
    };

    #[tokio::test]
    async fn disk_writer_finishes_accepted_write_after_request_is_cancelled() {
        with_temp_dir(|data_dir| {
            let data_dir = data_dir.to_path_buf();

            async move {
                let config = DiskBufferConfigBuilder::from_path(data_dir)
                    .build()
                    .expect("disk buffer config should be valid");
                let (writer, mut reader) =
                    Buffer::<SizedRecord>::from_config(config, BufferUsageHandle::noop())
                        .await
                        .expect("disk buffer should initialize");
                let sender = DiskV2Sender::new(writer);
                let first = SizedRecord::new(64);
                let second = SizedRecord::new(96);

                // Enqueue the first write and then discard its response, exactly matching a caller
                // future being cancelled after the writer task accepted the command.
                let (response, response_rx) = oneshot::channel();
                sender
                    .commands
                    .send(DiskV2WriterCommand::Write {
                        item: first.clone(),
                        blocking: true,
                        response,
                    })
                    .await
                    .expect("writer task should accept the command");
                drop(response_rx);

                // No later writer command is needed to complete or publish the cancelled request.
                let first_read = tokio::time::timeout(Duration::from_secs(2), reader.next())
                    .await
                    .expect("first read should not stall")
                    .expect("first read should succeed");
                assert_eq!(first_read, Some(first));

                // Reusing the same writer after cancellation must assign a new ID rather than
                // resubmitting the first record under its old ID.
                assert_eq!(
                    sender
                        .write(second.clone(), true)
                        .await
                        .expect("second write should succeed"),
                    TryWriteOutcome::Written
                );
                sender.flush().await.expect("writer flush should succeed");

                let second_read = tokio::time::timeout(Duration::from_secs(2), reader.next())
                    .await
                    .expect("second read should not stall")
                    .expect("second read should succeed");

                assert_eq!(second_read, Some(second));
            }
        })
        .await;
    }

    #[tokio::test]
    async fn disk_writer_stops_waiting_for_reader_when_last_sender_is_dropped() {
        with_temp_dir(|data_dir| {
            let data_dir = data_dir.to_path_buf();

            async move {
                let config = DiskBufferConfigBuilder::from_path(data_dir)
                    .max_buffer_size(4096)
                    .max_data_file_size(1024)
                    .max_record_size(1024)
                    .build()
                    .expect("disk buffer config should be valid");
                let (writer, _reader) =
                    Buffer::<SizedRecord>::from_config(config, BufferUsageHandle::noop())
                        .await
                        .expect("disk buffer should initialize");
                let sender = DiskV2Sender::new(writer);

                let mut blocked_item = None;
                for _ in 0..100 {
                    let item = SizedRecord::new(128);
                    match sender
                        .write(item, false)
                        .await
                        .expect("nonblocking write should succeed")
                    {
                        TryWriteOutcome::Written => {}
                        TryWriteOutcome::Full(item) => {
                            blocked_item = Some(item);
                            break;
                        }
                        TryWriteOutcome::Dropped => panic!("record should fit in the buffer"),
                    }
                }
                let blocked_item = blocked_item.expect("writes should fill the buffer");

                let (response, response_rx) = oneshot::channel();
                sender
                    .commands
                    .send(DiskV2WriterCommand::Write {
                        item: blocked_item,
                        blocking: true,
                        response,
                    })
                    .await
                    .expect("writer task should accept the blocking command");

                // Closing the final sender is the topology shutdown signal. It must interrupt the
                // reader-progress wait without cancelling any file I/O already in progress.
                drop(sender);

                let result = tokio::time::timeout(Duration::from_secs(2), response_rx)
                    .await
                    .expect("writer task should observe shutdown")
                    .expect("writer task should return the command response");
                assert!(matches!(
                    result,
                    Err(crate::variants::disk_v2::WriterError::Shutdown)
                ));
            }
        })
        .await;
    }
}
