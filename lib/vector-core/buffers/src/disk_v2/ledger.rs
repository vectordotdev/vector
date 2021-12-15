use std::{
    fmt, io,
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU16, AtomicU64, AtomicUsize, Ordering},
    time::Instant,
};

use bytecheck::CheckBytes;
use bytes::BytesMut;
use crossbeam_utils::atomic::AtomicCell;
use fslock::LockFile;
use memmap2::{MmapMut, MmapOptions};
use rkyv::{with::Atomic, Archive, Serialize};
use snafu::{ResultExt, Snafu};
use tokio::{
    fs::{self, OpenOptions},
    io::AsyncWriteExt,
    sync::Notify,
};

use crate::buffer_usage_data::BufferUsageHandle;

use super::{
    backed_archive::BackedArchive,
    common::{DiskBufferConfig, MAX_FILE_ID},
    ser::SerializeError,
};

/// Error that occurred during calls to [`Ledger`].
#[derive(Debug, Snafu)]
pub enum LedgerLoadCreateError {
    /// A general I/O error occurred.
    ///
    /// Generally, I/O errors should only occur when flushing the ledger state and the underlying
    /// ledger file has been corrupted or altered in some way outside of this process.  As the
    /// ledger is fixed in size, and does not grow during the life of the process, common errors
    /// such as running out of disk space will not typically be relevant (or possible) here.
    #[snafu(display("ledger I/O error: {}", source))]
    Io { source: io::Error },

    /// The ledger is already opened by another Vector process.
    ///
    /// Advisory locking is used to prevent other Vector processes from concurrently opening the
    /// same buffer, but bear in mind that this does not prevent other processes or users from
    /// modifying the ledger file in a way that could cause undefined behavior during buffer operation.
    #[snafu(display(
        "failed to lock buffer.lock; is another Vector process running and using this buffer?"
    ))]
    LedgerLockAlreadyHeld,

    /// The ledger state was unable to be deserialized.
    ///
    /// This should only occur if the ledger file was modified or truncated out of the Vector
    /// process.  In rare situations, if the ledger state type (`LedgerState`, here in ledger.rs)
    /// was modified, then the layout may now be out-of-line with the structure as it exists on disk.
    ///
    /// We have many strongly-worded warnings to not do this unless a developer absolutely knows
    /// what they're doing, but it is still technically a possibility. :)
    #[snafu(display("failed to deserialize ledger from buffer: {}", reason))]
    FailedToDeserialize { reason: String },

    /// The ledger state was unable to be serialized.
    ///
    /// This only occurs when initially creating a new buffer where the ledger state has not yet
    /// been written to disk.  During normal operation, the ledger is memory-mapped directly and so
    /// serialization does not occur.
    ///
    /// This error is likely only to occur if the process is unable to allocate memory for the
    /// buffers required for the serialization step.
    #[snafu(display("failed to serialize ledger to buffer: {}", reason))]
    FailedToSerialize { reason: String },
}

/// Ledger state.
///
/// Stores the relevant information related to both the reader and writer.  Gets serailized and
/// stored on disk, and is managed via a memory-mapped file.
///
/// # Warning
///
/// - Do not add fields to this struct.
/// - Do not remove fields from this struct.
/// - Do not change the type of fields in this struct.
/// - Do not change the order of fields this struct.
///
/// Doing so will change the serialized representation.  This will break things.
///
/// Do not do any of the listed things unless you _absolutely_ know what you're doing. :)
#[derive(Archive, Serialize, Debug)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct LedgerState {
    /// Total number of records persisted in this buffer.
    #[with(Atomic)]
    total_records: AtomicU64,
    /// Total size of all data files used by this buffer.
    #[with(Atomic)]
    total_buffer_size: AtomicU64,
    /// Next record ID to use when writing a record.
    #[with(Atomic)]
    writer_next_record_id: AtomicU64,
    /// The current data file ID being written to.
    #[with(Atomic)]
    writer_current_data_file_id: AtomicU16,
    /// The current data file ID being read from.
    #[with(Atomic)]
    reader_current_data_file_id: AtomicU16,
    /// The last record ID read by the reader.
    #[with(Atomic)]
    reader_last_record_id: AtomicU64,
}

impl Default for LedgerState {
    fn default() -> Self {
        Self {
            total_records: AtomicU64::new(0),
            total_buffer_size: AtomicU64::new(0),
            // First record written is always 1, so that our default of 0 for
            // `reader_last_record_id` ensures we start up in a state of "alright, waiting to read
            // record #1 next".
            writer_next_record_id: AtomicU64::new(1),
            writer_current_data_file_id: AtomicU16::new(0),
            reader_current_data_file_id: AtomicU16::new(0),
            reader_last_record_id: AtomicU64::new(0),
        }
    }
}

impl ArchivedLedgerState {
    pub(super) fn increment_records(&self, record_size: u64) {
        self.total_records.fetch_add(1, Ordering::AcqRel);
        self.total_buffer_size
            .fetch_add(record_size, Ordering::AcqRel);
    }

    pub(super) fn decrement_records(&self, record_len: u64, total_record_size: u64) {
        self.total_records.fetch_sub(record_len, Ordering::AcqRel);
        self.total_buffer_size
            .fetch_sub(total_record_size, Ordering::AcqRel);
    }

    /// Gets the total number of records in the buffer.
    pub fn get_total_records(&self) -> u64 {
        self.total_records.load(Ordering::Acquire)
    }

    pub(super) fn decrement_total_records(&self, amount: u64) {
        self.total_records.fetch_sub(amount, Ordering::AcqRel);
    }

    /// Gets the total number of bytes for all records in the buffer.
    ///
    /// This number will often disagree with the size of files on disk, as data files are deleted
    /// only after being read entirely, and are simply appended to when they are not yet full.  This
    /// leads to behavior where writes and reads will change this value only by the size of the
    /// records being written and read, while data files on disk will grow incrementally, and be
    /// deleted in full.
    pub fn get_total_buffer_size(&self) -> u64 {
        self.total_buffer_size.load(Ordering::Acquire)
    }

    pub(super) fn decrement_total_buffer_size(&self, amount: u64) {
        self.total_buffer_size.fetch_sub(amount, Ordering::AcqRel);
    }

    fn get_current_writer_file_id(&self) -> u16 {
        self.writer_current_data_file_id.load(Ordering::Acquire)
    }

    fn get_next_writer_file_id(&self) -> u16 {
        (self.get_current_writer_file_id() + 1) % MAX_FILE_ID
    }

    pub(super) fn increment_writer_file_id(&self) {
        self.writer_current_data_file_id
            .store(self.get_next_writer_file_id(), Ordering::Release);
    }

    pub(super) fn get_next_writer_record_id(&self) -> u64 {
        self.writer_next_record_id.load(Ordering::Acquire)
    }

    pub(super) fn increment_next_writer_record_id(&self) {
        self.writer_next_record_id.fetch_add(1, Ordering::AcqRel);
    }

    fn get_current_reader_file_id(&self) -> u16 {
        self.reader_current_data_file_id.load(Ordering::Acquire)
    }

    fn get_next_reader_file_id(&self) -> u16 {
        (self.get_current_reader_file_id() + 1) % MAX_FILE_ID
    }

    fn get_offset_reader_file_id(&self, offset: u16) -> u16 {
        self.get_current_reader_file_id().wrapping_add(offset) % MAX_FILE_ID
    }

    fn increment_reader_file_id(&self) {
        self.reader_current_data_file_id
            .store(self.get_next_reader_file_id(), Ordering::Release);
    }

    pub(super) fn get_last_reader_record_id(&self) -> u64 {
        self.reader_last_record_id.load(Ordering::Acquire)
    }

    pub(super) fn set_last_reader_record_id(&self, id: u64) {
        self.reader_last_record_id.store(id, Ordering::Release);
    }

    #[cfg(test)]
    pub unsafe fn unsafe_set_writer_next_record_id(&self, id: u64) {
        // UNSAFETY:
        // The atomic operation itself is inherently safe, but adjusting the record IDs manually is
        // _unsafe_ because it messes with the continuity of record IDs from the perspective of the
        // reader.
        //
        // This is exclusively used under test to make it possible to check certain edge cases, as
        // writing enough records to actually increment it to the maximum value would take longer
        // than any of us will be alive.
        //
        // Despite it being test-only, we're really amping up the "this is only for testing!" factor
        // by making it an actual `unsafe` function, and putting "unsafe" in the name. :)
        self.writer_next_record_id.store(id, Ordering::Release);
    }

    #[cfg(test)]
    pub unsafe fn unsafe_set_reader_last_record_id(&self, id: u64) {
        // UNSAFETY:
        // The atomic operation itself is inherently safe, but adjusting the record IDs manually is
        // _unsafe_ because it messes with the continuity of record IDs from the perspective of the
        // reader.
        //
        // This is exclusively used under test to make it possible to check certain edge cases, as
        // writing enough records to actually increment it to the maximum value would take longer
        // than any of us will be alive.
        //
        // Despite it being test-only, we're really amping up the "this is only for testing!" factor
        // by making it an actual `unsafe` function, and putting "unsafe" in the name. :)
        self.reader_last_record_id.store(id, Ordering::Release);
    }
}

/// Tracks the internal state of the buffer.
pub struct Ledger {
    // Buffer configuration.
    config: DiskBufferConfig,
    // Advisory lock for this buffer directory.
    ledger_lock: LockFile,
    // Ledger state.
    state: BackedArchive<MmapMut, LedgerState>,
    // Notifier for reader-related progress.
    reader_notify: Notify,
    // Notifier for writer-related progress.
    writer_notify: Notify,
    // Tracks when writer has fully shutdown.
    writer_done: AtomicBool,
    // Number of pending record acknowledgements that have yeet to be consumed by the reader.
    pending_acks: AtomicUsize,
    // The file ID offset of the reader past the acknowledged reader file ID.
    unacked_reader_file_id_offset: AtomicU16,
    // Last flush of all unflushed files: ledger, data file, etc.
    last_flush: AtomicCell<Instant>,
    // Tracks usage data about the buffer.
    usage_handle: BufferUsageHandle,
}

impl Ledger {
    /// Gets the configuration for the buffer that this ledger represents.
    pub fn config(&self) -> &DiskBufferConfig {
        &self.config
    }

    /// Gets the internal ledger state.
    ///
    /// This is the information persisted to disk.
    pub fn state(&self) -> &ArchivedLedgerState {
        self.state.get_archive_ref()
    }

    /// Gets the current reader file ID.
    ///
    /// This is internally adjusted to compensate for the fact that the reader can read far past
    /// the latest acknowledge record/data file, and so is not representative of where the reader
    /// would start reading from if the process crashed or was abruptly stopped.
    pub fn get_current_reader_file_id(&self) -> u16 {
        let unacked_offset = self.unacked_reader_file_id_offset.load(Ordering::Acquire);
        self.state().get_offset_reader_file_id(unacked_offset)
    }

    /// Gets the current writer file ID.
    pub fn get_current_writer_file_id(&self) -> u16 {
        self.state().get_current_writer_file_id()
    }

    /// Gets the next writer file ID.
    ///
    /// This is purely a future-looking operation i.e. what would the file ID be if it was
    /// incremented from its current value.  It does not alter the current writer file ID.
    #[cfg(test)]
    pub fn get_next_writer_file_id(&self) -> u16 {
        self.state().get_next_writer_file_id()
    }

    /// Gets the current reader and writer file IDs.
    ///
    /// Similar to [`get_current_reader_file_id`], the file ID returned for the reader compensates
    /// for the acknowledgement state of the reader.
    pub fn get_current_reader_writer_file_id(&self) -> (u16, u16) {
        let reader = self.get_current_reader_file_id();
        let writer = self.get_current_writer_file_id();

        (reader, writer)
    }

    /// Gets the current reader data file path, accounting for the unacknowledged offset.
    pub fn get_current_reader_data_file_path(&self) -> PathBuf {
        self.get_data_file_path(self.get_current_reader_file_id())
    }

    /// Gets the current writer data file path.
    pub fn get_current_writer_data_file_path(&self) -> PathBuf {
        self.get_data_file_path(self.get_current_reader_file_id())
    }

    /// Gets the next writer data file path.
    pub fn get_next_writer_data_file_path(&self) -> PathBuf {
        self.get_data_file_path(self.state().get_next_writer_file_id())
    }

    /// Gets the data file path for an arbitrary file ID.
    pub fn get_data_file_path(&self, file_id: u16) -> PathBuf {
        self.config
            .data_dir
            .join(format!("buffer-data-{}.dat", file_id))
    }

    /// Waits for a signal from the reader that progress has been made.
    ///
    /// This will only occur when a record is read, which may allow enough space (below the maximum
    /// configured buffer size) for a write to occur, or similarly, when a data file is deleted.
    #[cfg_attr(test, instrument(skip(self), level = "trace"))]
    pub async fn wait_for_reader(&self) {
        self.reader_notify.notified().await;
    }

    /// Waits for a signal from the writer that progress has been made.
    ///
    /// This will occur when a record is written, or when a new data file is created.
    #[cfg_attr(test, instrument(skip(self), level = "trace"))]
    pub async fn wait_for_writer(&self) {
        self.writer_notify.notified().await;
    }

    /// Notifies all tasks waiting on progress by the reader.
    #[cfg_attr(test, instrument(skip(self), level = "trace"))]
    pub fn notify_reader_waiters(&self) {
        self.reader_notify.notify_one();
    }

    /// Notifies all tasks waiting on progress by the writer.
    #[cfg_attr(test, instrument(skip(self), level = "trace"))]
    pub fn notify_writer_waiters(&self) {
        self.writer_notify.notify_one();
    }

    /// Tracks the statistics of a successful write.
    pub fn track_write(&self, record_size: u64) {
        self.state().increment_records(record_size);
        self.usage_handle
            .increment_received_event_count_and_byte_size(1, record_size);
    }

    /// Tracks the statistics of multiple successful reads.
    pub fn track_reads(&self, record_len: u64, total_record_size: u64) {
        self.state()
            .decrement_records(record_len, total_record_size);
        self.usage_handle
            .increment_sent_event_count_and_byte_size(record_len, total_record_size);
    }

    /// Marks the writer as finished.
    ///
    /// If the writer was not yet marked done, `false` is returned.  Otherwise, `true` is returned,
    /// and the caller should handle any necessary logic for closing the writer.
    pub fn mark_writer_done(&self) -> bool {
        self.writer_done
            .compare_exchange_weak(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// Returns `true` if the writer was marked as done.
    pub fn is_writer_done(&self) -> bool {
        self.writer_done.load(Ordering::Acquire)
    }

    /// Increments the pending acknowledgement counter by the given amount.
    pub fn increment_pending_acks(&self, amount: usize) {
        self.pending_acks.fetch_add(amount, Ordering::AcqRel);
    }

    /// Consumes the full amount of pending acknowledgements, and resets the counter to zero.
    pub fn consume_pending_acks(&self) -> usize {
        self.pending_acks.swap(0, Ordering::AcqRel)
    }

    /// Increments the unacknowledged reader file ID.
    ///
    /// As further described in `increment_acked_reader_file_id`, the underlying value here allows
    /// the reader to read ahead of a data file, even if it hasn't been durably processed yet.
    pub fn increment_unacked_reader_file_id(&self) {
        self.unacked_reader_file_id_offset
            .fetch_add(1, Ordering::AcqRel);
    }

    /// Increments the acknowledged reader file ID.
    ///
    /// As records may be read and stored for a small period of time (batching in a sink, etc), we
    /// cannot truly say that we have durably processed a record until the caller acknowledges the
    /// record.  However, if we always waited for an acknowledgement, then each read could be forced
    /// to wait for multiple seconds.  Such a design would clearly be unusable.
    ///
    /// Instead, we allow the reader to move ahead of the latest acknowledged record by tracking
    /// their current file ID and acknowledged file ID separately.  Once all records in a file have
    /// been acknowledged, the data file can be deleted and the reader file ID can be durably
    /// stored in the ledger.
    ///
    /// Callers use [`increment_unacked_reader_file_id`] to move to the next data file without
    /// tracking that the previous data file has been durably processed and can be deleted, and
    /// [`increment_acked_reader_file_id`] is the reciprocal function which tracks the highest data
    /// file that _has_ been durably processed.
    ///
    /// Since the unacked file ID is simply a relative offset to the acked file ID, we decrement it
    /// here to keep the "current" file ID stable.
    pub fn increment_acked_reader_file_id(&self) {
        self.state().increment_reader_file_id();

        // We ignore the return value because when the value is already zero, we don't want to do an
        // update, so we return `None`, which causes `fetch_update` to return `Err`.  It's not
        // really an error, we just wanted to avoid the extra atomic compare/exchange.
        //
        // Basically, this call is actually infallible for our purposes.
        let _ = self.unacked_reader_file_id_offset.fetch_update(
            Ordering::Release,
            Ordering::Relaxed,
            |n| {
                if n == 0 {
                    None
                } else {
                    Some(n - 1)
                }
            },
        );
    }

    /// Determines whether or not all files should be flushed/fsync'd to disk.
    ///
    /// In the case of concurrent callers when the flush deadline has been exceeded, only one caller
    /// will get a return value of `true`, and the others will receive `false`.  The caller that
    /// receives `true` is responsible for flushing the necessary files.
    pub fn should_flush(&self) -> bool {
        let last_flush = self.last_flush.load();
        if last_flush.elapsed() > self.config.flush_interval
            && self
                .last_flush
                .compare_exchange(last_flush, Instant::now())
                .is_ok()
        {
            return true;
        }

        false
    }

    /// Flushes the memory-mapped file backing the ledger to disk.
    ///
    /// This operation is synchronous.
    ///
    /// # Errors
    ///
    /// If there is an error while flushing the ledger to disk, an error variant will be returned
    /// describing the error.
    pub(super) fn flush(&self) -> io::Result<()> {
        self.state.get_backing_ref().flush()
    }

    fn synchronize_buffer_usage(&self) {
        let initial_buffer_events = self.state().get_total_records();
        let initial_buffer_size = self.state().get_total_buffer_size();
        self.usage_handle
            .increment_received_event_count_and_byte_size(
                initial_buffer_events,
                initial_buffer_size,
            );
    }

    /// Loads or creates a ledger for the given [`DiskBufferConfig`].
    ///
    /// If the ledger file does not yet exist, a default ledger state will be created and persisted
    /// to disk.  Otherwise, the ledger file on disk will be loaded and verified.
    ///
    /// # Errors
    ///
    /// If there is an error during either serialization of the new, default ledger state, or
    /// deserializing existing data in the ledger file, or generally during the underlying I/O
    /// operations, an error variant will be returned describing the error.
    #[cfg_attr(test, instrument(level = "trace"))]
    pub(super) async fn load_or_create(
        config: DiskBufferConfig,
        usage_handle: BufferUsageHandle,
    ) -> Result<Ledger, LedgerLoadCreateError> {
        // Create our containing directory if it doesn't already exist.
        fs::create_dir_all(&config.data_dir).await.context(Io)?;

        // Acquire an exclusive lock on our lock file, which prevents another Vector process from
        // loading this buffer and clashing with us.  Specifically, though: this does _not_ prevent
        // another process from messing with our ledger files, or any of the data files, etc.
        let ledger_lock_path = config.data_dir.join("buffer.lock");
        let mut ledger_lock = LockFile::open(&ledger_lock_path).context(Io)?;
        if !ledger_lock.try_lock().context(Io)? {
            return Err(LedgerLoadCreateError::LedgerLockAlreadyHeld);
        }

        // Open the ledger file, which may involve creating it if it doesn't yet exist.
        let ledger_path = config.data_dir.join("buffer.db");
        let mut ledger_handle = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&ledger_path)
            .await
            .context(Io)?;

        // If we just created the ledger file, then we need to create the default ledger state, and
        // then serialize and write to the file, before trying to load it as a memory-mapped file.
        let ledger_metadata = ledger_handle.metadata().await.context(Io)?;
        let ledger_len = ledger_metadata.len();
        if ledger_len == 0 {
            let mut buf = BytesMut::new();
            loop {
                match BackedArchive::from_value(&mut buf, LedgerState::default()) {
                    Ok(archive) => {
                        ledger_handle
                            .write_all(archive.get_backing_ref())
                            .await
                            .context(Io)?;
                        break;
                    }
                    Err(SerializeError::FailedToSerialize(reason)) => {
                        return Err(LedgerLoadCreateError::FailedToSerialize { reason })
                    }
                    // Our buffer wasn't big enough, but that's OK!  Resize it and try again.
                    Err(SerializeError::BackingStoreTooSmall(_, min_len)) => buf.resize(min_len, 0),
                }
            }
        }

        // Load the ledger state by memory-mapping the ledger file, and zero-copy deserializing our
        // ledger state back out of it.
        let ledger_handle = ledger_handle.into_std().await;
        let ledger_mmap = unsafe { MmapOptions::new().map_mut(&ledger_handle).context(Io)? };

        let ledger_state = match BackedArchive::from_backing(ledger_mmap) {
            // Deserialized the ledger state without issue from an existing file.
            Ok(backed) => backed,
            // Either invalid data, or the buffer doesn't represent a valid ledger structure.
            Err(e) => {
                return Err(LedgerLoadCreateError::FailedToDeserialize {
                    reason: e.into_inner(),
                })
            }
        };

        // Create the ledger object, and synchronize the buffer statistics with the buffer usage
        // handle.  This handles making sure we account for the starting size of the buffer, and
        // what not.
        let ledger = Ledger {
            config,
            ledger_lock,
            state: ledger_state,
            reader_notify: Notify::new(),
            writer_notify: Notify::new(),
            writer_done: AtomicBool::new(false),
            pending_acks: AtomicUsize::new(0),
            unacked_reader_file_id_offset: AtomicU16::new(0),
            last_flush: AtomicCell::new(Instant::now()),
            usage_handle,
        };
        ledger.synchronize_buffer_usage();

        Ok(ledger)
    }
}

impl fmt::Debug for Ledger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ledger")
            .field("config", &self.config)
            .field("ledger_lock", &self.ledger_lock)
            .field("state", &self.state.get_archive_ref())
            .field("reader_notify", &self.reader_notify)
            .field("writer_notify", &self.writer_notify)
            .field("pending_acks", &self.pending_acks.load(Ordering::Acquire))
            .field(
                "unacked_reader_file_id_offset",
                &self.unacked_reader_file_id_offset.load(Ordering::Acquire),
            )
            .field("writer_done", &self.writer_done.load(Ordering::Acquire))
            .field("last_flush", &self.last_flush)
            .finish()
    }
}
