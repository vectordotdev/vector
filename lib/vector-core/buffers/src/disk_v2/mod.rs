//! # Disk Buffer v2: Sequential File I/O Boogaloo.
//!
//! This disk buffer implementation is a reimplementation of the LevelDB-based disk buffer code that
//! already exists, but seeks to increase performance and reliability, while reducing the amount of
//! external code and hard-to-expose tunables.
//!
//! ## Design constraints
//!
//! These constraints, or more often, invariants, are the groundwork for ensuring that the design
//! can stay simple and understandable:
//! - buffer can grow to a maximum of ~8TB in total size
//! - data files do not exceed 128MB
//! - more more than 65,536 data files can exist at any given time
//! - all records are checksummed (CRC32C)
//! - all records are written sequentially/contiguous, and do not span over multiple data files
//! - writers create and write to data files, while readers read from and delete data files
//! - endianness of the files is based on the host system (we don't support loading the buffer files
//!   on a system with different endianness)
//!
//! ## On-disk layout
//!
//! At a high-level, records that are written end up in one of many underlying data files, while the
//! ledger file -- number of records, writer and reader positions, etc -- is stored in a separate
//! file.  Data files function primarily with a "last process who touched it" ownership model: the
//! writer always creates new files, and the reader deletes files when they have been fully read.
//!
//! ### Record structure
//! Records are packed together with a relatively simple pseudo-structure:
//!   record:
//!     record_len: uint32
//!     checksum: uint32 (CRC32C of record_id + payload)
//!     record_id: uint64
//!     payload: uint8[]
//!
//! We say pseudo-structure because we serialize these records to disk using `rkyv`, a zero-copy
//! deserialization library which focuses on the speed of reading values by writing them to storage
//! in a way that allows them to be "deserialized" without any copies, which means the layout of
//! struct fields matches their in-memory representation rather than the intuitive, packed structure
//! we might expect to see if we wrote only the bytes needed for each field, without any extra
//! padding or alignment.
//!
//! This represents a small amount of extra space overhead per record, but is beneficial to us as we
//! avoid a more formal deserialization step, with scratch buffers and memory copies.
//!     
//! ### Writing records
//!
//! Records are added to a data file sequentially, and contiguously, with no gaps or data alignment
//! adjustments, excluding the padding/alignment used by `rkyv` itself to allow for zero-copy
//! deserialization. This continues until adding another would exceed the configured data file size
//! limit.  When this occurs, the current data file is flushed and synchronized to disk, and a new
//! data file will be open.
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
//! Additionally, if the configured maximum data size would be exceeded, then a writer will wait for
//! the amount to drop (when a reader deletes a data file) before proceeding.
//!
//! ### Ledger structure
//!
//! Likewise, the ledger file consists of a simplified structure that is optimized for being
//! shared via a memory-mapped file interface between the writer and reader.  Like the record
//! structure, the below is a pseudo-structure as we use `rkyv` for the ledger, and so the on-disk
//! layout will be slightly different:
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
//! - implement specific error types so that we can return more useful errors than just wrapped I/O errors
use std::{
    io::{self, ErrorKind},
    path::Path,
    sync::Arc,
    time::Duration,
};

use tokio::fs::{self, File, OpenOptions};

mod backed_archive;
mod common;
mod ledger;
mod record;
mod record_reader;
mod record_writer;
mod ser;

use self::{
    common::{DATA_FILE_MAX_RECORD_SIZE, DATA_FILE_TARGET_MAX_SIZE},
    ledger::Ledger,
    record::ArchivedRecord,
    record_reader::{RecordEntry, RecordReader},
    record_writer::RecordWriter,
};

struct WriteState {
    ledger: Arc<Ledger>,
    writer: Option<RecordWriter<File>>,
    data_file_size: u64,
    target_data_file_size: u64,
    max_record_size: usize,
}

impl WriteState {
    pub fn new(ledger: Arc<Ledger>, target_data_file_size: u64, max_record_size: usize) -> Self {
        Self {
            ledger,
            writer: None,
            data_file_size: 0,
            target_data_file_size,
            max_record_size,
        }
    }

    fn track_write(&mut self, bytes_written: u64) {
        self.data_file_size += bytes_written;
        self.ledger.track_write(bytes_written);
    }

    fn can_write(&mut self) -> bool {
        self.data_file_size < self.target_data_file_size
    }

    fn reset(&mut self) {
        self.writer = None;
        self.data_file_size = 0;
    }

    pub async fn ensure_ready_for_write(&mut self) -> io::Result<()> {
        // If our data file is already open, and it has room left, then we're good here.  Otherwise,
        // flush everything and reset ourselves so that we can open the next data file for writing.
        let mut should_open_next = false;
        if self.writer.is_some() {
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
                        if file_len >= self.target_data_file_size {
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

                    self.writer = Some(RecordWriter::new(data_file));
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

    pub async fn write_record(&mut self, payload: &[u8]) -> io::Result<()> {
        let _ = self.ensure_ready_for_write().await?;

        let id = self.ledger.state().acquire_next_writer_record_id();
        let n = self
            .writer
            .as_mut()
            .unwrap()
            .write_record(id, payload)
            .await?;

        // Update the metadata now that we've written the record.
        self.track_write(n as u64);

        Ok(())
    }

    pub async fn flush(&mut self) -> io::Result<()> {
        // We always flush the `BufWriter` when this is called, but we don't always flush to disk or
        // flush the ledger.
        if let Some(writer) = self.writer.as_mut() {
            let _ = writer.flush().await?;
            self.ledger.notify_writer_waiters();
        }

        if self.ledger.should_flush() {
            if let Some(writer) = self.writer.as_mut() {
                let _ = writer.sync_all().await?;
            }

            self.ledger.flush().await
        } else {
            Ok(())
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
        if record_buf.len() > self.inner.max_record_size {
            return Err(io::Error::new(io::ErrorKind::Other, "record too large"));
        }

        self.inner.write_record(record_buf).await
    }

    pub async fn commit(self) -> io::Result<()> {
        self.inner.flush().await
    }
}

pub struct Writer {
    state: WriteState,
}

impl Writer {
    fn new(ledger: Arc<Ledger>, target_data_file_size: u64, max_record_size: usize) -> Self {
        let state = WriteState::new(ledger, target_data_file_size, max_record_size);
        Writer { state }
    }

    pub async fn ensure_ready_for_write(&mut self) -> io::Result<()> {
        self.state.ensure_ready_for_write().await
    }

    pub fn transaction(&mut self) -> WriteTransaction<'_> {
        WriteTransaction {
            inner: &mut self.state,
        }
    }
}

pub struct Reader {
    ledger: Arc<Ledger>,
    data_file: Option<RecordReader<File>>,
    last_reader_record_id: u64,
}

impl Reader {
    fn new(ledger: Arc<Ledger>) -> Self {
        let last_reader_record_id = ledger.state().get_last_reader_record_id();
        Reader {
            ledger,
            data_file: None,
            last_reader_record_id,
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

            self.data_file = Some(RecordReader::new(data_file));
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

    pub async fn next(&mut self) -> io::Result<&ArchivedRecord<'_>> {
        let token = loop {
            let _ = self.ensure_ready_for_read().await?;
            let reader = self
                .data_file
                .as_mut()
                .expect("reader was ensured to be ready");

            let current_writer_file_id = self.ledger.state().get_current_writer_file_id();
            let current_reader_file_id = self.ledger.state().get_current_reader_file_id();

            // Try reading a record.  If there wasn't enough data to read the length-delimiter, we
            // get back `None`, and we fall through to waiting for the writer to notify us before
            // trying again.

            // Try reading a record, which if successful, gives us a token to actually read/get a
            // reference to the record.  This is a slightly-tricky song-and-dance due rustc not yet
            // fully understanding mutable borrows when conditional control flow is involved.
            match reader.try_next_record().await? {
                // Not even enough data to read a length delimiter, so we need to wait for ther
                // writer to signal us that there's some actual data to read.
                None => {}
                // A length-delimited payload was read, but we failed to deserialize it as a valid
                // record, or we deseralized it and the checksum was invalid.  Either way, we're not
                // sure the rest of the data file is even valid, so roll to the next file.
                //
                // TODO: Right now, we're following the previous logic of not knowing where to find
                // the start of the next record, but since we're using a length-delimited framing
                // now, we could conceivably try one more time and if _that_ fails, then we roll to
                // the next data file.
                //
                // This really depends, I suppose, on what the likelihood is that we could have
                // invalid data on disk that can be deserialized as the backing data for an archived
                // record _and_ could also pass the checksum validation.  It seems incredibly
                // unlikely, but then again, we would also be parsing the payload as something else
                // at the next layer up,, so it would also have to be valid for _that_, which just
                // seems exceedingly unlikely.
                //
                // We would, at least, need to add some checks to the length delimiter, etc, to
                // detect clearly impossible situations i.e. lengths greater than our configured
                // record size limit, etc.  If we got anything like that, the reader might just
                // stall trying to read usize::MAX number of bytes, or whatever.
                //
                // As I type this all out, we're also theoretically vulnerable to that right now on
                // the very first read, not just after encountering our first known-to-be-corrupted
                // record.
                Some(RecordEntry::Corrupted) | Some(RecordEntry::FailedDeserialization(_)) => {
                    let _ = self.roll_to_next_data_file().await?;
                }
                // We got a valid record, so keep the token.
                Some(RecordEntry::Valid(token)) => break token,
            };

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
        };

        // We got a read token, so our record is present in the reader, and now we can actually read
        // it out and return a reference to it.
        self.update_reader_last_record_id(token.record_id());
        let reader = self
            .data_file
            .as_mut()
            .expect("reader was ensured to be ready");
        reader.read_record(token).await
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

        let writer = Writer::new(
            Arc::clone(&ledger),
            DATA_FILE_TARGET_MAX_SIZE,
            DATA_FILE_MAX_RECORD_SIZE,
        );
        let reader = Reader::new(ledger);

        Ok((writer, reader))
    }
}
