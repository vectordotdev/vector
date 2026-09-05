use std::{
    fmt, io,
    sync::{Arc, Mutex as StdMutex},
    time::Duration,
};

use futures::future::BoxFuture;
use tokio::{
    sync::{Mutex, Notify},
    time::sleep,
};
use vector_common::finalization::EventStatus;

use crate::{
    Bufferable,
    buffer_usage_data::BufferUsageHandle,
    variants::disk_v2::{
        self, CapacityProgress, Filesystem, ProductionFilesystem, TryWriteOutcome,
    },
};

#[cfg(test)]
use crate::variants::disk_v2::tests::model::filesystem::TestFilesystem;

const CAPACITY_RETRY_INITIAL: Duration = Duration::from_millis(100);
const CAPACITY_RETRY_MAX: Duration = Duration::from_secs(5);

#[derive(Debug)]
enum CapacityState {
    Ready,
    Retrying,
    Failed(TerminalError),
}

#[derive(Clone, Debug)]
struct TerminalError {
    kind: io::ErrorKind,
    raw_os_error: Option<i32>,
    message: String,
}

impl From<&io::Error> for TerminalError {
    fn from(error: &io::Error) -> Self {
        Self {
            kind: error.kind(),
            raw_os_error: error.raw_os_error(),
            message: error.to_string(),
        }
    }
}

impl TerminalError {
    fn to_io_error(&self) -> io::Error {
        // io::Error can preserve either a raw OS error or a custom message, but not both.
        // Prefer the raw error when present so callers retain its platform classification.
        match self.raw_os_error {
            Some(raw_os_error) => io::Error::from_raw_os_error(raw_os_error),
            None => io::Error::new(self.kind, self.message.clone()),
        }
    }
}

#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct CapacityBlockedHook {
    pub(crate) detected: Notify,
    pub(crate) resume: Notify,
}

struct UnownedFinalizers(Option<vector_common::finalization::EventFinalizerGroups>);

impl UnownedFinalizers {
    fn take<T: Bufferable>(item: &mut T) -> Self {
        Self(Some(item.take_finalizer_groups()))
    }

    fn restore<T: Bufferable>(mut self, item: &mut T) {
        item.merge_finalizer_groups(self.0.take().expect("finalizers must exist"));
    }
}

impl Drop for UnownedFinalizers {
    fn drop(&mut self) {
        if let Some(finalizers) = self.0.as_mut() {
            finalizers.update_status(EventStatus::Errored);
        }
    }
}

trait SenderFilesystem<T>: Filesystem + fmt::Debug + Clone + Sized + 'static
where
    T: Bufferable,
    Self::File: Unpin,
{
    fn try_write_record(
        writer: &mut disk_v2::BufferWriter<T, Self>,
        item: T,
    ) -> BoxFuture<'_, Result<TryWriteOutcome<T>, disk_v2::WriterError<T>>>;

    fn try_flush(
        writer: &mut disk_v2::BufferWriter<T, Self>,
    ) -> BoxFuture<'_, io::Result<CapacityProgress>>;

    fn retry_capacity(
        writer: &mut disk_v2::BufferWriter<T, Self>,
    ) -> BoxFuture<'_, io::Result<CapacityProgress>>;
}

macro_rules! impl_sender_filesystem {
    ($filesystem:ty) => {
        impl<T: Bufferable> SenderFilesystem<T> for $filesystem {
            fn try_write_record(
                writer: &mut disk_v2::BufferWriter<T, Self>,
                item: T,
            ) -> BoxFuture<'_, Result<TryWriteOutcome<T>, disk_v2::WriterError<T>>> {
                Box::pin(writer.try_write_record(item))
            }

            fn try_flush(
                writer: &mut disk_v2::BufferWriter<T, Self>,
            ) -> BoxFuture<'_, io::Result<CapacityProgress>> {
                Box::pin(writer.try_flush())
            }

            fn retry_capacity(
                writer: &mut disk_v2::BufferWriter<T, Self>,
            ) -> BoxFuture<'_, io::Result<CapacityProgress>> {
                Box::pin(writer.retry_capacity())
            }
        }
    };
}

impl_sender_filesystem!(ProductionFilesystem);
#[cfg(test)]
impl_sender_filesystem!(TestFilesystem);

#[derive(Debug)]
pub struct DiskV2Sender<T, FS = ProductionFilesystem>
where
    T: Bufferable,
    FS: Filesystem,
    FS::File: fmt::Debug + Unpin,
{
    writer: Mutex<disk_v2::BufferWriter<T, FS>>,
    pub(super) usage: BufferUsageHandle,
    capacity_state: StdMutex<CapacityState>,
    capacity_notify: Notify,
    #[cfg(test)]
    capacity_blocked_hook: StdMutex<Option<Arc<CapacityBlockedHook>>>,
}

struct WriterProgressOnCancel<'a, T, FS>
where
    T: Bufferable,
    FS: SenderFilesystem<T>,
    FS::File: fmt::Debug + Unpin,
{
    state: &'a Arc<DiskV2Sender<T, FS>>,
    armed: bool,
}

impl<'a, T, FS> WriterProgressOnCancel<'a, T, FS>
where
    T: Bufferable,
    FS: SenderFilesystem<T>,
    FS::File: fmt::Debug + Unpin,
{
    fn new(state: &'a Arc<DiskV2Sender<T, FS>>) -> Self {
        Self { state, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl<T, FS> Drop for WriterProgressOnCancel<'_, T, FS>
where
    T: Bufferable,
    FS: SenderFilesystem<T>,
    FS::File: fmt::Debug + Unpin,
{
    fn drop(&mut self) {
        if self.armed {
            self.state.start_cancellation_retry();
        }
    }
}

#[allow(private_bounds)]
impl<T, FS> DiskV2Sender<T, FS>
where
    T: Bufferable,
    FS: SenderFilesystem<T>,
    FS::File: fmt::Debug + Unpin,
{
    pub(super) fn new(writer: disk_v2::BufferWriter<T, FS>) -> Self {
        let usage = writer.usage_handle();
        Self {
            writer: Mutex::new(writer),
            usage,
            capacity_state: StdMutex::new(CapacityState::Ready),
            capacity_notify: Notify::new(),
            #[cfg(test)]
            capacity_blocked_hook: StdMutex::new(None),
        }
    }

    #[cfg(test)]
    async fn pause_after_capacity_blocked_for_test(&self) {
        let hook = self.capacity_blocked_hook.lock().expect("poisoned").clone();
        if let Some(hook) = hook {
            hook.detected.notify_waiters();
            hook.resume.notified().await;
        }
    }

    fn is_capacity_blocked(&self) -> bool {
        matches!(
            *self.capacity_state.lock().expect("poisoned"),
            CapacityState::Retrying
        )
    }

    fn start_capacity_retry(self: &Arc<Self>) {
        let should_start = {
            let mut capacity_state = self.capacity_state.lock().expect("poisoned");
            match *capacity_state {
                CapacityState::Ready => {
                    *capacity_state = CapacityState::Retrying;
                    true
                }
                CapacityState::Retrying | CapacityState::Failed(_) => false,
            }
        };
        if should_start {
            self.capacity_notify.notify_waiters();
            self.spawn_capacity_retry_driver(None);
        }
    }

    fn start_cancellation_retry(self: &Arc<Self>) {
        let (retained_driver, classifier) = {
            let mut capacity_state = self.capacity_state.lock().expect("poisoned");
            match *capacity_state {
                CapacityState::Ready => {
                    *capacity_state = CapacityState::Retrying;
                    (Some(Arc::clone(self)), None)
                }
                CapacityState::Retrying => (None, Some(Arc::clone(self))),
                CapacityState::Failed(_) => return,
            }
        };
        if let Some(state) = retained_driver {
            self.capacity_notify.notify_waiters();
            self.spawn_capacity_retry_driver(Some(state));
        } else if let Some(state) = classifier {
            Self::classify_cancelled_retry(state);
        }
    }

    fn classify_cancelled_retry(state: Arc<Self>) {
        tokio::spawn(async move {
            let result = {
                let mut writer = state.writer.lock().await;
                state.retry_capacity_once(&mut writer).await
            };

            let _ = result;
        });
    }

    async fn retry_capacity_once(
        &self,
        writer: &mut disk_v2::BufferWriter<T, FS>,
    ) -> Option<io::Result<CapacityProgress>> {
        if !matches!(
            *self.capacity_state.lock().expect("poisoned"),
            CapacityState::Retrying
        ) {
            return None;
        }

        let result = FS::retry_capacity(writer).await;
        match &result {
            Ok(CapacityProgress::Ready) if !writer.is_capacity_blocked() => {
                let mut capacity_state = self.capacity_state.lock().expect("poisoned");
                if matches!(*capacity_state, CapacityState::Retrying) {
                    *capacity_state = CapacityState::Ready;
                    self.capacity_notify.notify_waiters();
                }
            }
            Err(error) => {
                writer.fail_pending_write();
                error!(%error, "Disk buffer capacity retry failed.");
                let mut capacity_state = self.capacity_state.lock().expect("poisoned");
                if matches!(*capacity_state, CapacityState::Retrying) {
                    *capacity_state = CapacityState::Failed(TerminalError::from(error));
                    self.capacity_notify.notify_waiters();
                }
            }
            Ok(
                CapacityProgress::Blocked
                | CapacityProgress::WaitingForReader
                | CapacityProgress::Ready,
            ) => {}
        }
        Some(result)
    }

    fn spawn_capacity_retry_driver(self: &Arc<Self>, mut retained_state: Option<Arc<Self>>) {
        let weak_state = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut delay = CAPACITY_RETRY_INITIAL;
            loop {
                let Some(state) = retained_state.clone().or_else(|| weak_state.upgrade()) else {
                    return;
                };
                let reader_progress = {
                    let writer = state.writer.lock().await;
                    writer.reader_progress_waiter()
                };
                drop(state);
                tokio::select! {
                    () = sleep(delay) => {}
                    () = reader_progress => {}
                }
                let Some(state) = retained_state.clone().or_else(|| weak_state.upgrade()) else {
                    return;
                };
                let result = {
                    let mut writer = state.writer.lock().await;
                    state.retry_capacity_once(&mut writer).await
                };
                let Some(result) = result else {
                    return;
                };
                // A cancellation can leave a production syscall running after its caller drops.
                // Keep its sender alive through the first retry result, then return to weak ownership.
                retained_state = None;
                match result {
                    Ok(CapacityProgress::Ready) => {
                        if state.is_capacity_blocked() {
                            continue;
                        }
                        return;
                    }
                    Ok(CapacityProgress::Blocked) => {
                        delay = delay.saturating_mul(2).min(CAPACITY_RETRY_MAX);
                    }
                    Ok(CapacityProgress::WaitingForReader) => {}
                    Err(_) => {
                        return;
                    }
                }
            }
        });
    }

    async fn wait_for_capacity(&self) -> crate::Result<()> {
        let notified = self.capacity_notify.notified();
        tokio::pin!(notified);
        loop {
            notified.as_mut().enable();
            match &*self.capacity_state.lock().expect("poisoned") {
                CapacityState::Ready => return Ok(()),
                CapacityState::Failed(error) => {
                    return Err(error.to_io_error().into());
                }
                CapacityState::Retrying => {}
            }
            notified.as_mut().await;
            notified.set(self.capacity_notify.notified());
        }
    }

    pub(super) async fn try_send_record(
        self: &Arc<Self>,
        item: T,
    ) -> crate::Result<TryWriteOutcome<T>> {
        let mut item = item;
        let finalizers = UnownedFinalizers::take(&mut item);
        if let CapacityState::Failed(error) = &*self.capacity_state.lock().expect("poisoned") {
            return Err(error.to_io_error().into());
        }
        finalizers.restore(&mut item);

        let pre_count = item.event_count() as u64;
        let pre_size = item.size_of() as u64;
        let Some(mut item) = item.filter_unencodable() else {
            self.usage
                .increment_received_event_count_and_byte_size(pre_count, pre_size);
            self.usage
                .increment_dropped_event_count_and_byte_size(pre_count, pre_size, false);
            return Ok(TryWriteOutcome::Dropped);
        };
        if item.event_count() as u64 != pre_count {
            let dropped_events = pre_count - item.event_count() as u64;
            let dropped_bytes = pre_size.saturating_sub(item.size_of() as u64);
            self.usage
                .increment_received_event_count_and_byte_size(dropped_events, dropped_bytes);
            self.usage.increment_dropped_event_count_and_byte_size(
                dropped_events,
                dropped_bytes,
                false,
            );
        }

        let finalizers = UnownedFinalizers::take(&mut item);
        let notified = self.capacity_notify.notified();
        tokio::pin!(notified);
        let mut writer = loop {
            notified.as_mut().enable();
            match &*self.capacity_state.lock().expect("poisoned") {
                CapacityState::Ready => {}
                CapacityState::Retrying => {
                    finalizers.restore(&mut item);
                    return Ok(TryWriteOutcome::Full(item));
                }
                CapacityState::Failed(error) => {
                    return Err(error.to_io_error().into());
                }
            }

            tokio::select! {
                biased;
                () = notified.as_mut() => {
                    notified.set(self.capacity_notify.notified());
                }
                writer = self.writer.lock() => {
                    match &*self.capacity_state.lock().expect("poisoned") {
                        CapacityState::Ready => break writer,
                        CapacityState::Retrying => {
                            drop(writer);
                            finalizers.restore(&mut item);
                            return Ok(TryWriteOutcome::Full(item));
                        }
                        CapacityState::Failed(error) => {
                            return Err(error.to_io_error().into());
                        }
                    }
                }
            }
        };
        if writer.is_capacity_blocked() {
            #[cfg(test)]
            self.pause_after_capacity_blocked_for_test().await;
            self.start_capacity_retry();
            drop(writer);
            finalizers.restore(&mut item);
            return Ok(TryWriteOutcome::Full(item));
        }

        finalizers.restore(&mut item);
        let mut progress_on_cancel = WriterProgressOnCancel::new(self);
        let result = FS::try_write_record(&mut writer, item).await;
        progress_on_cancel.disarm();
        let outcome = result.inspect_err(|_error| {
            error!("Disk buffer writer has encountered an unrecoverable error.");
        })?;
        let blocked = writer.is_capacity_blocked();
        if blocked {
            #[cfg(test)]
            self.pause_after_capacity_blocked_for_test().await;
            self.start_capacity_retry();
        }
        drop(writer);
        Ok(outcome)
    }

    pub(super) async fn send_record(
        self: &Arc<Self>,
        item: T,
    ) -> crate::Result<TryWriteOutcome<T>> {
        let mut item = item;
        loop {
            match self.try_send_record(item).await? {
                TryWriteOutcome::Written => return Ok(TryWriteOutcome::Written),
                TryWriteOutcome::Dropped => return Ok(TryWriteOutcome::Dropped),
                TryWriteOutcome::Pending => {
                    self.wait_for_capacity().await?;
                    return Ok(TryWriteOutcome::Written);
                }
                TryWriteOutcome::Full(mut returned) => {
                    let finalizers = UnownedFinalizers::take(&mut returned);
                    item = returned;
                    if self.is_capacity_blocked() {
                        self.wait_for_capacity().await?;
                    } else {
                        let writer = self.writer.lock().await;
                        let reader_progress = writer.reader_progress_waiter();
                        drop(writer);
                        reader_progress.await;
                    }
                    finalizers.restore(&mut item);
                }
            }
        }
    }

    pub(super) async fn flush(self: &Arc<Self>, block: bool) -> crate::Result<()> {
        loop {
            let retrying = {
                match &*self.capacity_state.lock().expect("poisoned") {
                    CapacityState::Ready => false,
                    CapacityState::Retrying => true,
                    CapacityState::Failed(error) => {
                        return Err(error.to_io_error().into());
                    }
                }
            };
            if retrying {
                if !block {
                    return Ok(());
                }
                self.wait_for_capacity().await?;
                continue;
            }

            let mut writer = self.writer.lock().await;
            let retrying = {
                match &*self.capacity_state.lock().expect("poisoned") {
                    CapacityState::Ready => false,
                    CapacityState::Retrying => true,
                    CapacityState::Failed(error) => {
                        return Err(error.to_io_error().into());
                    }
                }
            };
            if retrying {
                drop(writer);
                if !block {
                    return Ok(());
                }
                self.wait_for_capacity().await?;
                continue;
            }

            // This guard only covers filesystem I/O. Cancelling while waiting for capacity must not
            // start a second capacity retry driver.
            let mut progress_on_cancel = WriterProgressOnCancel::new(self);
            let result = FS::try_flush(&mut writer).await;
            progress_on_cancel.disarm();
            let progress = result.map_err(|error| -> Box<dyn std::error::Error + Send + Sync> {
                error!("Disk buffer writer has encountered an unrecoverable error.");
                error.into()
            })?;

            if matches!(progress, CapacityProgress::Blocked) {
                // Publish Retrying before releasing the writer, so another flush cannot race in
                // and issue another filesystem call before observing the capacity episode.
                self.start_capacity_retry();
            }
            drop(writer);

            if matches!(progress, CapacityProgress::Blocked) && block {
                self.wait_for_capacity().await?;
                continue;
            }
            return Ok(());
        }
    }

    #[cfg(test)]
    pub(crate) fn start_normal_capacity_retry_for_test(self: &Arc<Self>) {
        self.start_capacity_retry();
    }

    #[cfg(test)]
    pub(crate) fn start_cancellation_retry_for_test(self: &Arc<Self>) {
        self.start_cancellation_retry();
    }

    #[cfg(test)]
    pub(crate) fn set_capacity_blocked_hook(&self, hook: Arc<CapacityBlockedHook>) {
        *self.capacity_blocked_hook.lock().expect("poisoned") = Some(hook);
    }
}

#[cfg(test)]
impl<T: Bufferable> DiskV2Sender<T, TestFilesystem> {
    pub(crate) async fn next_writer_data_file_path(&self) -> std::path::PathBuf {
        self.writer.lock().await.next_writer_data_file_path()
    }
}
