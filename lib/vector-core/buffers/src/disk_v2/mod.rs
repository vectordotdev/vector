//! # Disk Buffer v2: Sequential File I/O Boogaloo.
//!
//! This disk buffer implementation is a reimplementation of the LEvelDB-based disk buffer code that
//! already exists, but seeks to increase performance and reliability, while reducing the amount of
//! external code and hard-to-expose tunables.
//!
//! ## Design constraints
//!
//! These constraints, or more often, invariants, are the groundwork for ensuring that the design
//! can stay simple and understandable://!
//! - buffer can grow to a maximum of ~8TB in total size
//! - data files do not exceed 128MB
//! - more more than 65,536 data files can exist at any given time
//! - all headers (ledger, data file) are written in network byte order (big endian) when integers
//!   are involved
//! - all records are checksummed (CRC32C)
//! - all records are written sequentially/contiguous, and do not span over multiple data files
//! - writers create and write to data files, while readers read from and delete data files
//!
//! ## On-disk layout
//!
//! At a high-level, records that are written end up in one of many underlying data files, while the
//! ledger file -- number of records, writer and reader positions, etc -- is stored in a separate
//! file.  Data files function primarily with a "last process who touched it" ownership model: the
//! writer always creates new files, and the reader deletes files when they have been fully read.
//!
//! ### Record structure
//! Internally, data files consist of a simplified structure that is optimized for the ring buffer
//! use case.  Records are packed together with a minimalistic layout:
//!
//!   record:
//!     checksum: uint32 // CRC32C of ID + payload
//!     length: uint32
//!     id: uint64
//!     payload: uint8[length]
//!
//! ### Writing records
//!
//! Records are added to a data file sequentially, and contiguously, with no gaps or data alignment
//! adjustments. This continues until adding another would exceed the configured data file size limit.
//! When this occurs, the current data file is flushed and synchronized to disk, and a new data file will be open.
//!
//! If the number of data files open exceeds the maximum (65,536), or if the total data file size
//! limit is exceeded, the writer will wait until enough space has been freed such that the record
//! can be written.  As data files are only deleted after being read entirely, this means that space
//! is recovered in increments of the target data file size, which is 128MB.  Thus, the minimum size
//! for a buffer must be equal to or greater than the target size of a single data file.
//! Additionally, as data files are uniquely named based on an incrementing integer, of which will
//! wrap around at 65,536 (2^16), the maximum data file size in total for a given buffer is ~8TB (6
//! 5k files * 128MB).
//!
//! ### Ledger structure
//!
//! Likewise, the ledger file consists of a simplified structure that is optimized for being
//! shared via a memory-mapped file interface between the writer and reader:
//!
//!   buffer.db:
//!     [total record count - unsigned 64-bit integer]
//!     [total buffer size - unsigned 64-bit integer]
//!     [next record ID - unsigned 64-bit integer]
//!     [writer current data file ID - unsigned 16-bit integer]
//!     [reader current data file ID - unsigned 16-bit integer]
//!     [reader last record ID - unsigned 64-bit integer]
//!
//! As the disk buffer structure is meant to emulate a ring buffer, most of the bookkeeping resolves around the
//! writer and reader being able to quickly figure out where they left off.  Record and data file
//! IDs are simply rolled over when they reach the maximum of their data type, aand are incremented
//! monontonically as new data files are created, rather than trying to always allocate from the
//! lowest available ID.
//!
//! Additionally, record IDs are allocated in the same way: monotonic, sequential, and will wrap
//! when they reach the maximum value for the data type.  For record IDs, however, this would mean
//! reaching 2^64, which will take a really, really, really long time.
//!
//! # Implementation TODOs:
//! - implement an advisory lock around the ledger so multiple processes can't collide (`fs2` crate)
//! - wire up seeking to the last (according to the ledger) read record when first creating a reader
//! - make file size limits configurable for testing purposes (we could easily write 2-3x of the
//!   128MB target under test, but it'd be faster if we didn't have to, and doing that would take a
//!   while to exercise certain logic like file ID wraparound)
//! - actually limit the total file usage size (add logic to update the total file size in the ledger)
//! - test what happens on file ID rollover
//! - test what happens on writer wrapping and wanting to open current data file being read by reader
//! - test what happens when record ID rolls over
//! - figure out a way to deal with restarting a process where some updates were unflushed, and the
//!   writer ends up reusing some record IDs that get double read by the reader, which itself has
//!   logic for detecting non-monotonic record IDs
use std::{
    convert::TryInto,
    io::{self, ErrorKind},
    ops::Range,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU16, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use bytes::{BufMut, Bytes, BytesMut};
use crc32fast::Hasher;
use crossbeam_utils::atomic::AtomicCell;
use memmap2::{MmapMut, MmapOptions};
use tokio::{
    fs::{self, File, OpenOptions},
    io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter},
    sync::{Mutex, Notify},
};

const LEDGER_FILE_SIZE: usize = 36;
// We don't want data files to be bigger than 128MB, but we might end up overshooting slightly.
const DATA_FILE_TARGET_MAX_SIZE: u64 = 128 * 1024 * 1024;
// There's no particular reason that _has_ to be 8MB, it's just a simple default we've chosen here.
const DATA_FILE_MAX_RECORD_SIZE: u32 = 8 * 1024 * 1024;
// Record header: record checksum (u32) + record length (u32) + record ID (u64).
const DATA_FILE_RECORD_HEADER_SIZE: u64 = 16;

#[derive(Debug)]
struct LedgerState {
    // Total number of records persisted in this buffer.
    total_records: AtomicU64,
    // Total size of all data files used by this buffer.
    total_buffer_size: AtomicU64,
    // Next record ID to use when writing a record.
    writer_next_record_id: AtomicU64,
    // The current data file ID being written to.
    writer_current_data_file_id: AtomicU16,
    // The current data file ID being read from.
    reader_current_data_file_id: AtomicU16,
    // The last record ID read by the reader.
    reader_last_record_id: AtomicU64,
}

impl Default for LedgerState {
    fn default() -> Self {
        Self {
            total_records: Default::default(),
            total_buffer_size: Default::default(),
            // First record written is always 1, so that our defualt of 0 for
            // `reader_last_record_id` ensures we start up in a state of "alright, waiting to read
            // record #1 next".
            writer_next_record_id: 1.into(),
            writer_current_data_file_id: Default::default(),
            reader_current_data_file_id: Default::default(),
            reader_last_record_id: Default::default(),
        }
    }
}

impl LedgerState {
    pub fn acquire_next_writer_record_id(&self) -> u64 {
        self.writer_next_record_id.fetch_add(1, Ordering::AcqRel)
    }

    pub fn get_last_reader_record_id(&self) -> u64 {
        self.reader_last_record_id.load(Ordering::Acquire)
    }

    pub fn set_last_reader_record_id(&self, id: u64) {
        self.reader_last_record_id.store(id, Ordering::Release);
    }

    /// Gets the current writer file ID.
    pub fn get_current_writer_file_id(&self) -> u16 {
        self.writer_current_data_file_id.load(Ordering::Acquire)
    }

    /// Gets the next writer file ID.
    pub fn get_next_writer_file_id(&self) -> u16 {
        self.writer_current_data_file_id
            .load(Ordering::Acquire)
            .wrapping_add(1)
    }

    /// Increments the current writer file ID.
    pub fn increment_writer_file_id(&self) {
        self.writer_current_data_file_id
            .fetch_add(1, Ordering::AcqRel);
    }

    /// Gets the current reader file ID.
    pub fn get_current_reader_file_id(&self) -> u16 {
        self.reader_current_data_file_id.load(Ordering::Acquire)
    }

    /// Increments the current reader file ID.
    pub fn increment_reader_file_id(&self) {
        self.reader_current_data_file_id
            .fetch_add(1, Ordering::AcqRel);
    }

    pub fn serialize_to(&self, dst: &mut [u8]) {
        // CLARITY TODO: This is very ugly, and fragile due to field offsets.  It'd be nice if we
        // had a macro or something to make this a little more programmatic/repeatable/machine
        // checkable.  Given that we only have three structs which we serialize in this fashion,
        // though, that could be overkill.
        //
        // PERFORMANCE TODO: This is a nice, safe variant of pushing the state into the data file,
        // but I'm not sure if doing a pointer-level `memcpy` action would be meaningfully faster.
        let total_records = self.total_records.load(Ordering::SeqCst).to_be_bytes();
        let total_buffer_size = self.total_buffer_size.load(Ordering::SeqCst).to_be_bytes();
        let next_record_id = self
            .writer_next_record_id
            .load(Ordering::SeqCst)
            .to_be_bytes();
        let writer_current_data_file_id = self
            .writer_current_data_file_id
            .load(Ordering::SeqCst)
            .to_be_bytes();
        let reader_current_data_file_id = self
            .reader_current_data_file_id
            .load(Ordering::SeqCst)
            .to_be_bytes();
        let reader_last_record_id = self
            .reader_last_record_id
            .load(Ordering::SeqCst)
            .to_be_bytes();

        let mut src = Vec::new();
        src.extend_from_slice(&total_records[..]);
        src.extend_from_slice(&total_buffer_size[..]);
        src.extend_from_slice(&next_record_id[..]);
        src.extend_from_slice(&writer_current_data_file_id[..]);
        src.extend_from_slice(&reader_current_data_file_id[..]);
        src.extend_from_slice(&reader_last_record_id[..]);

        debug_assert!(dst.len() == LEDGER_FILE_SIZE);
        debug_assert!(src.len() == LEDGER_FILE_SIZE);
        dst.copy_from_slice(&src[..]);
    }

    pub fn deserialize_from(&mut self, src: &[u8]) {
        // CLARITY TODO: This is very ugly, and fragile due to field offsets.  It'd be nice if we
        // had a macro or something to make this a little more programmatic/repeatable/machine
        // checkable.  Given that we only have three structs which we serialize in this fashion,
        // though, that could be overkill.
        debug_assert!(src.len() == LEDGER_FILE_SIZE);

        self.total_records = src[..8]
            .try_into()
            .map(u64::from_be_bytes)
            .map(AtomicU64::new)
            .expect("should have had 8 bytes");
        self.total_buffer_size = src[8..16]
            .try_into()
            .map(u64::from_be_bytes)
            .map(AtomicU64::new)
            .expect("should have had 8 bytes");
        self.writer_next_record_id = src[16..24]
            .try_into()
            .map(u64::from_be_bytes)
            .map(AtomicU64::new)
            .expect("should have had 8 bytes");
        self.writer_current_data_file_id = src[24..26]
            .try_into()
            .map(u16::from_be_bytes)
            .map(AtomicU16::new)
            .expect("should have had 2 bytes");
        self.reader_current_data_file_id = src[26..28]
            .try_into()
            .map(u16::from_be_bytes)
            .map(AtomicU16::new)
            .expect("should have had 2 bytes");
        self.reader_last_record_id = src[28..36]
            .try_into()
            .map(u64::from_be_bytes)
            .map(AtomicU64::new)
            .expect("should have had 8 bytes");
    }
}

#[derive(Debug)]
struct Ledger {
    // Path to the data directory.
    data_dir: PathBuf,
    // Handle to the memory-mapped ledger file.
    ledger_mmap: Mutex<MmapMut>,
    // Ledger state.
    state: LedgerState,
    // Notifier for reader-related progress.
    reader_notify: Notify,
    // Notifier for writer-related progress.
    writer_notify: Notify,
    // Last flush of all unflushed files: ledger, data file, etc.
    last_flush: AtomicCell<Instant>,
    // How often flushes should occur.
    //
    // Flushes may occur more often as a data file filling up forcefully triggers a flush so that
    // all data is on-disk before moving on to the next data file.
    flush_interval: Duration,
}

impl Ledger {
    pub fn state(&self) -> &LedgerState {
        &self.state
    }

    pub fn get_current_reader_data_file_path(&self) -> PathBuf {
        self.get_data_file_path(self.state.get_current_reader_file_id())
    }

    pub fn get_current_writer_data_file_path(&self) -> PathBuf {
        self.get_data_file_path(self.state.get_current_writer_file_id())
    }

    pub fn get_next_writer_data_file_path(&self) -> PathBuf {
        self.get_data_file_path(self.state.get_next_writer_file_id())
    }

    pub fn get_data_file_path(&self, file_id: u16) -> PathBuf {
        self.data_dir.join(format!("buffer-data-{}.dat", file_id))
    }

    /// Waits for a signal from the reader that an entire data file has been read and subsequently deleted.
    pub async fn wait_for_reader(&self) {
        self.reader_notify.notified().await
    }

    /// Waits for a signal from the writer that data has been written to a data file, or that a new
    /// data file has been created.
    pub async fn wait_for_writer(&self) {
        self.writer_notify.notified().await
    }

    /// Notifies all tasks waiting on progress by the reader.
    pub fn notify_reader_waiters(&self) {
        self.reader_notify.notify_waiters()
    }

    /// Notifies all tasks waiting on progress by the writer.
    pub fn notify_writer_waiters(&self) {
        self.writer_notify.notify_waiters()
    }

    /// Determines whether or not all files should be flushed/fsync'd to disk.
    ///
    /// In the case of concurrent callers when the flush deadline has been exceeded, only one caller
    /// will get a return value of `true`, and the others will receive `false`.  The caller that
    /// receives `true` is responsible for flushing the necessary files.
    fn should_flush(&self) -> bool {
        let last_flush = self.last_flush.load();
        if last_flush.elapsed() > self.flush_interval {
            if let Ok(_) = self.last_flush.compare_exchange(last_flush, Instant::now()) {
                return true;
            }
        }

        false
    }

    async fn read_from_disk(&mut self) -> io::Result<()> {
        // TODO: this theoretically doesn't need to return a Result right now, let alone an
        // io::Result, but at some point we should likely be adding checksums and doing other
        // checks, so loading our state from disk would become a fallible operation

        // INVARIANT: We always create the ledger file with a size of LEDGER_FILE_SIZE, which never
        // changes over time.  We can be sure that the slice we take to read the ledger state will
        // be exactly LEDGER_FILE_SIZE bytes.
        let ledger_mmap = self.ledger_mmap.lock().await;
        let ledger_region = &ledger_mmap[..];
        debug_assert_eq!(ledger_region.len(), LEDGER_FILE_SIZE);
        self.state.deserialize_from(ledger_region);

        Ok(())
    }

    pub fn track_write(&self, bytes_written: u64) {
        self.state.total_records.fetch_add(1, Ordering::Release);
        self.state
            .total_buffer_size
            .fetch_add(bytes_written, Ordering::Release);
    }

    pub async fn flush(&self) -> io::Result<()> {
        // INVARIANT: We always create the ledger file with a size of LEDGER_FILE_SIZE, which never
        // changes over time.  We can be sure that the slice we take to write the ledger state will
        // be exactly LEDGER_FILE_SIZE bytes.
        let mut ledger_mmap = self.ledger_mmap.lock().await;
        let ledger_region = &mut ledger_mmap[..];
        debug_assert_eq!(ledger_region.len(), LEDGER_FILE_SIZE);
        self.state.serialize_to(ledger_region);

        ledger_mmap.flush()
    }

    pub async fn load_or_create<P>(data_dir: P, flush_interval: Duration) -> io::Result<Ledger>
    where
        P: AsRef<Path>,
    {
        let ledger_path = data_dir.as_ref().join("buffer.db");
        let ledger_handle = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&ledger_path)
            .await?;

        // If we're creating the ledger for the first time, ensure the file is the right size.
        let ledger_metadata = ledger_handle.metadata().await?;
        let ledger_len = ledger_metadata.len();
        let is_ledger_new = ledger_len == 0;
        if is_ledger_new {
            let _ = ledger_handle.set_len(LEDGER_FILE_SIZE as u64).await?;
        }

        let ledger_handle = ledger_handle.into_std().await;
        let ledger_mmap = unsafe { MmapOptions::new().map_mut(&ledger_handle)? };
        let mut ledger = Ledger {
            data_dir: data_dir.as_ref().to_owned(),
            ledger_mmap: Mutex::new(ledger_mmap),
            state: LedgerState::default(),
            reader_notify: Notify::new(),
            writer_notify: Notify::new(),
            last_flush: AtomicCell::new(Instant::now()),
            flush_interval,
        };

        // Don't load the ledger from disk if we just created it, otherwise we'll override the
        // default ledger state by deserializing from a bunch of zeroes.
        if !is_ledger_new {
            let _ = ledger.read_from_disk().await?;
        }

        Ok(ledger)
    }
}

struct WriteState {
    ledger: Arc<Ledger>,
    data_file: Option<BufWriter<File>>,
    data_file_size: u64,
    checksummer: Hasher,
}

impl WriteState {
    fn track_write(&mut self, bytes_written: u64) {
        self.data_file_size += bytes_written;
        self.ledger.track_write(bytes_written);
    }

    fn can_write(&mut self) -> bool {
        self.data_file_size < DATA_FILE_TARGET_MAX_SIZE
    }

    fn reset(&mut self) {
        self.data_file = None;
        self.data_file_size = 0;
    }

    pub async fn ensure_ready_for_write(&mut self) -> io::Result<()> {
        // If our data file is already open, and it has room left, then we're good here.  Otherwise,
        // flush everything and reset ourselves so that we can open the next data file for writing.
        let mut should_open_next = false;
        if self.data_file.is_some() {
            if self.can_write() {
                return Ok(());
            }

            // Our current data file is full, so we need to open a new one.  Signal to the loop
            // that we we want to try and open the next file, and not the current file,
            // essentially to avoid marking the writer as already having moved on to the next
            // file before we're sure it isn't already an existing file on disk waiting to be
            // read.
            //
            // We still flush ourselves to disk, etc, to make sure all of the data is there.
            should_open_next = true;
            let _ = self.flush().await?;

            self.reset();
        }

        loop {
            // Normally, readers will keep up with the writers, and so there will only ever be a
            // single data file or two on disk.  If there was an issue with a sink reading from this
            // buffer, though, we could conceivably have a stalled reader while the writer
            // progresses and continues to create new data file.
            //
            // At some point, the file ID will wrap around and the writer will want to open a "new"
            // file for writing that already exists: a previously-written file that has not been
            // read yet.
            //
            // In order to handle this situation, we loop here, trying to create the file.  Readers
            // are responsible deleting a file once they have read it entirely, so our first loop
            // iteration is the happy path, trying to create the new file.  If we can't create it,
            // we explicitly wait for the reader to signal that it has made writer-relevant
            // progress: in other words, that it has fully read and deleted a data file, in case we
            // were waiting for that to happen.
            let data_file_path = if should_open_next {
                self.ledger.get_next_writer_data_file_path()
            } else {
                self.ledger.get_current_writer_data_file_path()
            };
            let maybe_data_file = OpenOptions::new()
                .append(true)
                .create_new(true)
                .open(&data_file_path)
                .await;

            let file = match maybe_data_file {
                // We were able to create the file, so we're good to proceed.
                Ok(data_file) => Some((data_file, 0)),
                // We got back an error trying to open the file: might be that it already exists,
                // might be something else.
                Err(e) => match e.kind() {
                    // The file already exists, so it might have been a file we left off writing
                    // to, or it might be full.  Figure out which.
                    ErrorKind::AlreadyExists => {
                        // We open the file again, without the atomic "create new" behavior.  If we
                        // can do that successfully, we check its length.  Anything less than our
                        // target max file size indicates that it's either a partially-filled data
                        // file that we can pick back up, _or_ that the reader finished and deleted
                        // the file between our initial open attempt and this one.
                        //
                        // If the file is indeed "full", though, then we hand back `None`, which
                        // will force a wait on reader progress before trying again.
                        let data_file = OpenOptions::new()
                            .append(true)
                            .create(true)
                            .open(&data_file_path)
                            .await?;
                        let metadata = data_file.metadata().await?;
                        let file_len = metadata.len();
                        if file_len >= DATA_FILE_TARGET_MAX_SIZE {
                            None
                        } else {
                            Some((data_file, file_len))
                        }
                    }
                    // Legitimate I/O error with the operation, bubble this up.
                    _ => return Err(e),
                },
            };

            match file {
                // We successfully opened the file and it can be written to.
                Some((data_file, data_file_size)) => {
                    // Make sure the file is flushed to disk, especially if we just created it.
                    let _ = data_file.sync_all().await?;

                    self.data_file = Some(BufWriter::new(data_file));
                    self.data_file_size = data_file_size;

                    // If we opened the "next" data file, we need to increment the current writer
                    // file ID now to signal that the writer has moved on.
                    if should_open_next {
                        self.ledger.state().increment_writer_file_id();
                        self.ledger.notify_writer_waiters();
                    }

                    return Ok(());
                }
                // The file is still present and waiting for a reader to finish reading it in order
                // to delete it.  Wait until the reader signals progress and try again.
                None => self.ledger.wait_for_reader().await,
            }
        }
    }

    pub async fn write_record(&mut self, record_buf: &[u8]) -> io::Result<()> {
        let _ = self.ensure_ready_for_write().await?;

        let record_id = self.ledger.state().acquire_next_writer_record_id();

        let record_id = record_id.to_be_bytes();
        let record_checksum = self
            .generate_checksum(&record_id[..], record_buf)
            .to_be_bytes();
        let record_length = (record_buf.len() as u32).to_be_bytes();

        // Write our record header and data.
        let data_file = self.data_file.as_mut().expect("data file should be open");
        let _ = data_file.write_all(&record_checksum[..]).await?;
        let _ = data_file.write_all(&record_length[..]).await?;
        let _ = data_file.write_all(&record_id[..]).await?;
        let _ = data_file.write_all(record_buf).await?;

        // Update the metadata now that we've written the record.
        let bytes_written =
            record_buf.len() + record_checksum.len() + record_length.len() + record_id.len();
        self.track_write(bytes_written as u64);

        Ok(())
    }

    pub async fn flush(&mut self) -> io::Result<()> {
        if let Some(data_file) = self.data_file.as_mut() {
            let _ = data_file.flush().await?;
            let _ = data_file.get_mut().flush().await?;
            let _ = data_file.get_mut().sync_all().await?;
        }

        self.ledger.flush().await
    }

    pub async fn maybe_flush(&mut self) -> io::Result<()> {
        // We always flush the `BufWriter` when this is called, but we don't always flush to disk or
        // flush the ledger.
        if let Some(data_file) = self.data_file.as_mut() {
            let _ = data_file.flush().await?;
            let _ = data_file.get_mut().flush().await?;
            self.ledger.notify_writer_waiters();
        }

        if self.ledger.should_flush() {
            self.flush().await
        } else {
            Ok(())
        }
    }

    fn generate_checksum(&mut self, id: &[u8], payload: &[u8]) -> u32 {
        let mut checksummer = self.checksummer.clone();
        checksummer.reset();

        // Record ID is always in network order.
        checksummer.update(&id[..]);
        checksummer.update(payload);

        checksummer.finalize()
    }
}

impl From<Arc<Ledger>> for WriteState {
    fn from(ledger: Arc<Ledger>) -> Self {
        WriteState {
            ledger,
            data_file: None,
            data_file_size: 0,
            checksummer: Hasher::new(),
        }
    }
}

pub struct WriteTransaction<'a> {
    inner: &'a mut WriteState,
}

impl<'a> WriteTransaction<'a> {
    pub async fn write<R>(&mut self, record: R) -> io::Result<()>
    where
        R: AsRef<[u8]>,
    {
        let record_buf = record.as_ref();

        // Check that the record isn't bigger than the maximum record size.  This isn't a limitation
        // of writing to files, but mostly just common sense to have some reasonable upper bound.
        let buf_len = u32::try_from(record_buf.len()).map_err(|_| {
            io::Error::new(
                io::ErrorKind::Other,
                "record buf size bigger than 2^32 bytes!",
            )
        })?;
        if buf_len > DATA_FILE_MAX_RECORD_SIZE {
            return Err(io::Error::new(io::ErrorKind::Other, "record too large"));
        }

        self.inner.write_record(record_buf).await
    }

    pub async fn commit(self) -> io::Result<()> {
        let _ = self.inner.maybe_flush().await?;
        Ok(())
    }
}

pub struct Writer {
    ledger: Arc<Ledger>,
    state: WriteState,
}

impl Writer {
    fn new(ledger: Arc<Ledger>) -> Self {
        let state = WriteState::from(Arc::clone(&ledger));
        Writer { ledger, state }
    }

    pub fn total_records(&self) -> u64 {
        self.ledger.state.total_records.load(Ordering::Relaxed)
    }

    pub async fn ensure_ready_for_write(&mut self) -> io::Result<()> {
        self.state.ensure_ready_for_write().await
    }

    pub async fn maybe_flush(&mut self) -> io::Result<()> {
        self.state.maybe_flush().await
    }

    pub fn transaction(&mut self) -> WriteTransaction<'_> {
        WriteTransaction {
            inner: &mut self.state,
        }
    }
}

pub struct Reader {
    ledger: Arc<Ledger>,
    data_file: Option<BufReader<File>>,
    buf: BytesMut,
    buf_pos: usize,
    last_reader_record_id: u64,
    checksummer: Hasher,
}

impl Reader {
    fn new(ledger: Arc<Ledger>) -> Self {
        let last_reader_record_id = ledger.state().get_last_reader_record_id();
        Reader {
            ledger,
            data_file: None,
            buf: BytesMut::with_capacity(8192),
            buf_pos: 0,
            last_reader_record_id,
            checksummer: Hasher::new(),
        }
    }

    /// Switches the reader over to the next data file to read.
    async fn roll_to_next_data_file(&mut self) -> io::Result<()> {
        // Delete the current data file, and increment our reader file ID.
        self.data_file = None;

        // Delete the current data file, and increment our reader file ID.
        let data_file_path = self.ledger.get_current_reader_data_file_path();
        let _ = fs::remove_file(&data_file_path).await?;

        self.ledger.state().increment_reader_file_id();
        let _ = self.ledger.flush().await?;

        // Notify any waiting writers that we've deleted a data file, which they may be waiting on
        // because they're looking to reuse the file ID of the file we just finished reading.
        self.ledger.notify_reader_waiters();
        Ok(())
    }

    /// Resets the internal read buffer state, clearing any remaining data.
    ///
    /// Practically speaking, it isn't necessary to fully clear the buffer, because we use
    /// `self.buf_pos` to track initialized (read) bytes in the buffer, but this is more for peace
    /// of mind than anything else.
    fn reset_internal_read_buffer(&mut self) {
        self.buf.clear();
        self.buf_pos = 0;
    }

    /// Attempts to read an exact number of bytes from the underlying data file.
    ///
    /// If the exact number of bytes is not available, `None` is returned.  Partial reads are
    /// buffered internally and so multiple calls may be required before the read can be fulfilled.
    ///
    /// It is expected that the same value of `n` be used for all calls until the request is
    /// fulfilled.  For example, if 16 bytes must be read in order to parse the record header for
    /// the next record, then `n` should be passed as `16` until a successful call is made.
    ///
    /// Once a read is successful, an internal buffer position is incremented such that `n` bytes
    /// are "consumed" from the internal buffer.  The internal buffer that is used grows by the sum
    /// of `n` for successful reads, and must be reset via `self.reset_internal_read_buffer` in
    /// order to reuse the existing buffer capacity.
    async fn try_read_exact(&mut self, n: usize) -> io::Result<Option<Range<usize>>> {
        loop {
            // If our buffer already has enough bytes to fulfill the read, consume them.
            if self.buf.len() - self.buf_pos >= n {
                let start = self.buf_pos;
                self.buf_pos += n;

                return Ok(Some(start..self.buf_pos));
            }

            // We don't have enough buffered data to fulfill the request, so we need to do another
            // read.  We don't want to read more than we need, though, since `self.buf` is cleared
            // before every request, so figure out exactly how many more bytes we need to read, and
            // make sure we have the capacity to hold that many more bytes.
            let needed = n - (self.buf.len() - self.buf_pos);
            if needed > self.buf.capacity() {
                self.buf.reserve(needed);
            }

            // Try the read.  We split the buffer up specifically so that we hand over a `BytesMut`
            // that is the exact size of the additional number of bytes we need to read, ensuring we
            // don't read anything extra.  After the read, we splice the buffers back together.
            let read_n = {
                let unread_chunk = self.buf.split_off(self.buf.len());
                let mut needed_chunk = unread_chunk.limit(needed);

                let data_file = self
                    .data_file
                    .as_mut()
                    .expect("data file must be initialized");

                let result = data_file.read_buf(&mut needed_chunk).await?;

                let unread_chunk = needed_chunk.into_inner();
                self.buf.unsplit(unread_chunk);

                result
            };

            // If the read attempt hit EOF, we need to wait for the writer to signal us that more
            // data has been written, or that the data file has been closed, so we have to pass back
            // control to `next` at this point.
            if read_n == 0 {
                return Ok(None);
            }
        }
    }

    /// Ensures this reader is ready to attempt reading the next record.
    pub async fn ensure_ready_for_read(&mut self) -> io::Result<()> {
        // We have nothing to do if we already have a data file open.
        if self.data_file.is_some() {
            return Ok(());
        }

        // Try to open the current reader data file.  This might not _yet_ exist, in which case
        // we'll simply wait for the writer to signal to us that progress has been made, which
        // implies a data file existing.
        loop {
            let data_file_path = self.ledger.get_current_reader_data_file_path();
            let data_file = match File::open(&data_file_path).await {
                Ok(data_file) => data_file,
                Err(e) => match e.kind() {
                    ErrorKind::NotFound => {
                        self.ledger.wait_for_writer().await;
                        continue;
                    }
                    // This is a valid I/O error, so bubble that back up.
                    _ => return Err(e),
                },
            };

            self.data_file = Some(BufReader::new(data_file));
            return Ok(());
        }
    }

    fn update_reader_last_record_id(&mut self, record_id: u64) {
        let previous_id = self.last_reader_record_id;
        self.last_reader_record_id = record_id;

        let id_delta = record_id - previous_id;
        match id_delta {
            // IDs should always move forward by one.
            0 => panic!("delta should always be one or more"),
            // A normal read where the ID is, in fact, one higher than our last record ID.
            1 => self.ledger.state().set_last_reader_record_id(record_id),
            n => {
                // We've skipped records, likely due to detecting and invalid checksum and skipping
                // the rest of that file.  Now that we've successfully read another record, and
                // since IDs are sequential, we can determine how many records were skipped and emit
                // that as an event.
                //
                // If `n` is equal to `record_id`, that means the process restarted and we're
                // seeking to the last record that we marked ourselves as having read, so no issues.
                if n != record_id {
                    println!("skipped records; last {}, now {}", previous_id, record_id);

                    // TODO: This is where we would emit an actual metric to track the corrupted
                    // (and thus dropped) events we just skipped over.
                    let _corrupted_events = id_delta - 1;
                }
            }
        }
    }

    /// Seeks to the next record that the reader should read.
    ///
    /// Under normal operation, the writer next/reader last record IDs are staggered, such that
    /// in a fresh buffer, the "next" record ID for the writer to use when writing a record is
    /// `1`, and the "last" record ID for the reader to use when reading a record is `0`.  No
    /// seeking or adjusting of file cursors is necessary, as the writer/reader should move in
    /// lockstep, including when new data files are created.
    ///
    /// In cases where Vector has restarted, but the reader hasn't yet finished a file, we would
    /// open the correct data file for reading, but our file cursor would be at the very
    /// beginning, essentially pointed at the wrong record.  We read out records here until we
    /// reach a point where we've read up to the record right before `get_last_reader_record_id`.
    /// This ensures that a subsequent call to `next` is ready to read the correct record.
    async fn seek_to_next_record(&mut self) -> io::Result<()> {
        // We rely on `next` to close out the data file if we've actually reached the end, and we
        // also rely on it to reset the data file before trying to read, and we _also_ rely on it to
        // update `self.last_reader_record_id`, so basically... just keep reading records until we
        // get to the one we left off with last time.
        let last_reader_record_id = self.ledger.state().get_last_reader_record_id();
        while self.last_reader_record_id < last_reader_record_id {
            let _ = self.next().await?;
        }

        Ok(())
    }

    pub async fn next(&mut self) -> io::Result<(u64, Bytes)> {
        self.reset_internal_read_buffer();

        #[derive(Clone, Copy, Debug, PartialEq)]
        struct Header {
            checksum: u32,
            len: u32,
            id: u64,
        }

        #[derive(Clone, Copy, Debug, PartialEq)]
        enum State {
            NeedHeader,
            NeedPayload(Header),
        }

        let mut state = State::NeedHeader;
        loop {
            let _ = self.ensure_ready_for_read().await?;
            let current_writer_file_id = self.ledger.state().get_current_writer_file_id();
            let current_reader_file_id = self.ledger.state().get_current_reader_file_id();

            let next_state = match state {
                State::NeedHeader => {
                    match self
                        .try_read_exact(DATA_FILE_RECORD_HEADER_SIZE as usize)
                        .await?
                    {
                        Some(buf_range) => {
                            let buf = &self.buf[buf_range];

                            // TODO: We should have a centralized set of (de)serialization methods
                            // for `Header` instead of doing it manually like we're doing here.
                            let checksum = u32::from_be_bytes(buf[0..4].try_into().unwrap());
                            let len = u32::from_be_bytes(buf[4..8].try_into().unwrap());
                            let id = u64::from_be_bytes(buf[8..16].try_into().unwrap());

                            let header = Header { checksum, len, id };
                            State::NeedPayload(header)
                        }
                        // We don't have enough data for the header yet.
                        None => state,
                    }
                }
                State::NeedPayload(header) => match self.try_read_exact(header.len as usize).await?
                {
                    // We've read the entire payload, so verify the checksum, etc.
                    Some(buf_range) => {
                        let buf = &self.buf[buf_range];
                        let payload = Bytes::copy_from_slice(buf);

                        let mut checksummer = self.checksummer.clone();
                        checksummer.reset();

                        let record_id = header.id.to_be_bytes();
                        checksummer.update(&record_id[..]);
                        checksummer.update(&payload[..]);

                        let checksum = checksummer.finalize();
                        if checksum == header.checksum {
                            // The checksum was valid, so we can return this record to the caller.
                            self.update_reader_last_record_id(header.id);
                            return Ok((header.id, payload));
                        }

                        // The checksum was not valid.
                        //
                        // Since we append records to the data file without any sort of
                        // padding/alignment, it is impossible to figure out the true number of
                        // bytes that would have to be skipped to get to the next record.
                        //
                        // This means that we need to discard this data file and move to the next
                        // one.  This implies that we are potentially dropping records by skipping
                        // the rest of the data file.  The next time we successfully read a record,
                        // `update_reader_last_record_id` will detect if we've skipped records, and
                        // if so, how many, and emit the correct events to signal that it has
                        // happened.
                        //
                        // We don't need to throw an error, though as we can simply reset our state
                        // and try reading a record from the next data file.
                        //
                        // TODO: Since we would be waiting for the next data file to open while the
                        // writer is still writing, we might be dropping events on the floor
                        // _beyond_ all the events had been written at the time of our corrupted
                        // read.  Could we actually force the writer to also roll over to the next
                        // data file to give us a chance to minimize how many records we drop?
                        //
                        // TODO: If we try and drop the file while it's still open by the writer,
                        // does the writer continuing to write to that file mean it actually never
                        // gets deleted? And if it never gets deleted, then we'll eventually lock
                        // ourselves up when the reader hits this data file again.  It really does
                        // seem like we need to force the writer to move to the next file before we
                        // delete this one to ensure correctness. Is there a reasonable design for
                        // us to achieve that?
                        let _ = self.roll_to_next_data_file().await?;
                        State::NeedHeader
                    }
                    // The payload isn't available to read yet, so we stay in this state.
                    None => state,
                },
            };

            if next_state == State::NeedHeader {
                // Fundamentally, when we can't fully read the header during the attempted header
                // read, there's two possible scenarios at play:
                //
                // 1. we are entirely caught up to the writer
                // 2. we've hit the end of the data file and need to go to the next one
                //
                // In order to figure out which situation we're in, we load both the current reader
                // file ID and current writer file ID.  When we hit the above scenarios, our state
                // does not transition, so when we get to the end of the loop, we check if we're
                // still in the "need header" state.
                //
                // When we're in this state, we first "wait" for the writer to wake us up.  This
                // might be an existing buffered wakeup, or we might actually be waiting for the
                // next wakeup.  Regardless of which type of wakeup it is, we still end up checking
                // if the reader and writer file IDs that we loaded match.
                //
                // If the file IDs were identical, it would imply that reader is still on the
                // current writer data file.  We simply continue the loop in this case.  It may lead
                // to the same thing, being stuck in the "need header" state with an identical
                // reader/writer file ID, but that's OK, because it would mean we were actually
                // waiting for the writer to make progress now.  If the wakeupo was valid, due to
                // writer progress, then, well... we'd actually be able to read data.
                //
                // If the file IDs were not identical, we now know the writer has moved on.
                // Crucially, since we always flush our writes before waking up, including before
                // moving to a new file, then we know that if the reader/writer were not identical
                // at the start the loop, and we still ended up in the "need header" state, that we
                // have hit the actual end of the current reader data file, and need to move on.
                self.ledger.wait_for_writer().await;

                if current_writer_file_id != current_reader_file_id {
                    let _ = self.roll_to_next_data_file().await?;
                }
            }

            state = next_state;
        }
    }
}

pub struct Buffer;

impl Buffer {
    pub async fn from_path<P>(data_dir: P) -> io::Result<(Writer, Reader)>
    where
        P: AsRef<Path>,
    {
        let ledger = Ledger::load_or_create(data_dir, Duration::from_secs(1)).await?;
        let ledger = Arc::new(ledger);

        let writer = Writer::new(Arc::clone(&ledger));
        let reader = Reader::new(ledger);

        Ok((writer, reader))
    }
}
