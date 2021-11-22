use std::{
    cmp,
    io::{self, ErrorKind},
    marker::PhantomData,
    sync::Arc,
};

use crc32fast::Hasher;
use rkyv::{archived_root, AlignedVec};
use snafu::{ResultExt, Snafu};
use tokio::{
    fs::{self, File},
    io::{AsyncBufReadExt, AsyncRead, BufReader},
};

use crate::{bytes::DecodeBytes, Bufferable};

use super::{
    ledger::Ledger,
    record::{try_as_record_archive, Record, RecordStatus},
};

pub struct ReadToken(usize, u64);

impl ReadToken {
    pub fn record_size(&self) -> usize {
        self.0
    }

    pub fn record_id(&self) -> u64 {
        self.1
    }
}

#[derive(Debug, Snafu)]
pub enum ReaderError<T>
where
    T: Bufferable,
{
    #[snafu(display("read I/O error: {}", source))]
    Io { source: io::Error },
    #[snafu(display("failed to deserialize encoded record from buffer: {}", reason))]
    FailedToDeserialize { reason: String },
    #[snafu(display(
        "calculated checksum did not match the actual checksum: ({} vs {})",
        calculated,
        actual
    ))]
    InvalidChecksum { calculated: u32, actual: u32 },
    #[snafu(display("failed to decoded record: {:?}", source))]
    FailedToDecode {
        source: <T as DecodeBytes<T>>::Error,
    },
}

pub struct RecordReader<R, T> {
    reader: BufReader<R>,
    aligned_buf: AlignedVec,
    checksummer: Hasher,
    current_record_id: u64,
    _t: PhantomData<T>,
}

impl<R, T> RecordReader<R, T>
where
    R: AsyncRead + Unpin,
    T: Bufferable,
{
    pub fn new(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader),
            aligned_buf: AlignedVec::new(),
            checksummer: Hasher::new(),
            current_record_id: 0,
            _t: PhantomData,
        }
    }

    async fn read_length_delimiter(&mut self) -> Result<Option<usize>, ReaderError<T>> {
        loop {
            if self.reader.buffer().len() >= 4 {
                let length_buf = &self.reader.buffer()[..4];
                let length = length_buf
                    .try_into()
                    .expect("the slice is the length of a u32");
                self.reader.consume(4);

                return Ok(Some(u32::from_be_bytes(length) as usize));
            }

            let buf = self.reader.fill_buf().await.context(Io)?;
            if buf.is_empty() {
                return Ok(None);
            }
        }
    }

    async fn try_next_record(&mut self) -> Result<Option<ReadToken>, ReaderError<T>> {
        let record_len = match self.read_length_delimiter().await? {
            Some(len) => len,
            None => return Ok(None),
        };

        // Read in all of the bytes we need first.
        self.aligned_buf.clear();
        while self.aligned_buf.len() < record_len {
            let needed = record_len - self.aligned_buf.len();
            let buf = self.reader.fill_buf().await.context(Io)?;

            let available = cmp::min(buf.len(), needed);
            self.aligned_buf.extend_from_slice(&buf[..available]);
            self.reader.consume(available);
        }

        // Now see if we can deserialize our archived record from this.
        let buf = self.aligned_buf.as_slice();
        match try_as_record_archive(buf, &self.checksummer) {
            // TODO: do something in the error / corrupted cases; emit an error, increment an error
            // counter, yadda yadda. something.
            RecordStatus::FailedDeserialization(de) => Err(ReaderError::FailedToDeserialize {
                reason: de.into_inner(),
            }),
            RecordStatus::Corrupted { calculated, actual } => {
                Err(ReaderError::InvalidChecksum { calculated, actual })
            }
            RecordStatus::Valid(id) => {
                self.current_record_id = id;
                Ok(Some(ReadToken(4 + buf.len(), id)))
            }
        }
    }

    async fn read_record(&mut self, token: ReadToken) -> Result<T, ReaderError<T>> {
        if token.1 != self.current_record_id {
            panic!("using expired read token");
        }

        // SAFETY:
        // - `try_next_record` is the only method that can hand back a `ReadToken`
        // - we only get a `ReadToken` if there's a valid record in `self.aligned_buf`
        // - `try_next_record` does all the archive checks, checksum validation, etc
        let archived_record = unsafe { archived_root::<Record<'_>>(&self.aligned_buf) };

        T::decode(archived_record.payload()).context(FailedToDecode)
    }

    /// Consumes this `RecordReader`, returning the underlying reader.
    pub fn into_inner(self) -> R {
        self.reader.into_inner()
    }
}

pub struct Reader<T> {
    ledger: Arc<Ledger>,
    reader: Option<RecordReader<File, T>>,
    bytes_read: u64,
    last_reader_record_id: u64,
    ready_to_read: bool,
    _t: PhantomData<T>,
}

impl<T> Reader<T>
where
    T: Bufferable,
{
    pub(crate) fn new(ledger: Arc<Ledger>) -> Self {
        Reader {
            ledger,
            reader: None,
            bytes_read: 0,
            last_reader_record_id: 0,
            ready_to_read: false,
            _t: PhantomData,
        }
    }

    fn track_read(&mut self, record_size: u64) {
        self.bytes_read += record_size;
        self.ledger.track_read(record_size);
    }

    /// Switches the reader over to the next data file to read.
    async fn roll_to_next_data_file(&mut self) -> io::Result<()> {
        // Try and grab the file size once we're ready to roll over.  We use this to figure out if
        // we need to subtract some more bytes from the total buffer size.  If we rolled over due to
        // error, then we'd be keeping around bytes we'd never get to read, leaving the buffer size
        // value invalid.  This fixes that.
        //
        // TODO: this doesn't address record count, though
        if let Some(reader) = self.reader.take() {
            let file = reader.into_inner();
            let metadata = file.metadata().await?;

            // TODO: figure out if we ever hit a wraparound on file_size - self.bytes_read because
            // the equal read/write count on disk_v2_controlled_progress ending with ~0.8GB leftover
            // makes no sense at all.
            let file_size = metadata.len();
            let size_delta = file_size - self.bytes_read;
            self.ledger.state().decrement_total_buffer_size(size_delta);
        }

        // Delete the current data file, and increment our reader file ID.
        let data_file_path = self.ledger.get_current_reader_data_file_path();
        let _ = fs::remove_file(&data_file_path).await?;

        self.ledger.state().increment_reader_file_id();
        let _ = self.ledger.flush()?;

        // Notify any waiting writers that we've deleted a data file, which they may be waiting on
        // because they're looking to reuse the file ID of the file we just finished reading.
        self.ledger.notify_reader_waiters();
        Ok(())
    }

    /// Ensures this reader is ready to attempt reading the next record.
    async fn ensure_ready_for_read(&mut self) -> io::Result<()> {
        // We have nothing to do if we already have a data file open.
        if self.reader.is_some() {
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

            self.reader = Some(RecordReader::new(data_file));
            return Ok(());
        }
    }

    fn update_reader_last_record_id(&mut self, record_id: u64) {
        let previous_id = self.last_reader_record_id;
        self.last_reader_record_id = record_id;

        // Don't execute the ID delta logic when we're still in setup mode, which is where we would
        // be reading record IDs below our last read record ID.
        if !self.ready_to_read {
            return;
        }

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
                    println!(
                        "skipped records; last {}, now {} (delta={})",
                        previous_id, record_id, id_delta
                    );

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
    pub(crate) async fn seek_to_next_record(&mut self) -> Result<(), ReaderError<T>> {
        // We rely on `next` to close out the data file if we've actually reached the end, and we
        // also rely on it to reset the data file before trying to read, and we _also_ rely on it to
        // update `self.last_reader_record_id`, so basically... just keep reading records until we
        // get to the one we left off with last time.
        let last_reader_record_id = self.ledger.state().get_last_reader_record_id();
        while self.last_reader_record_id < last_reader_record_id {
            let _ = self.next().await?;
        }

        self.ready_to_read = true;

        Ok(())
    }

    /// Reads a record.
    pub async fn next(&mut self) -> Result<T, ReaderError<T>> {
        let token = loop {
            let _ = self.ensure_ready_for_read().await.context(Io)?;
            let reader = self
                .reader
                .as_mut()
                .expect("reader should exist after ensure_ready_for_read");

            let current_writer_file_id = self.ledger.state().get_current_writer_file_id();
            let current_reader_file_id = self.ledger.state().get_current_reader_file_id();

            // Try reading a record, which if successful, gives us a token to actually read/get a
            // reference to the record.  This is a slightly-tricky song-and-dance due to rustc not
            // yet fully understanding mutable borrows when conditional control flow is involved.
            match reader.try_next_record().await {
                // Not even enough data to read a length delimiter, so we need to wait for the
                // writer to signal us that there's some actual data to read.
                Ok(None) => {}
                // We got a valid record, so keep the token.
                Ok(Some(token)) => break token,
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
                Err(e) => match e {
                    // Invalid checksums and deserialization failures can't really be acted upon by
                    // the caller, but they might be expecting a read-after-write behavior, so we
                    // return the error to them after ensuring that we roll to the next file first.
                    e @ ReaderError::InvalidChecksum { .. }
                    | e @ ReaderError::FailedToDeserialize { .. } => {
                        let _ = self.roll_to_next_data_file().await.context(Io)?;
                        return Err(e);
                    }
                    // Nothing specific to do about an I/O error or decoding failure.
                    e => return Err(e),
                },
            };

            // Fundamentally, when `try_read_record` returns `None`, there's two possible scenarios:
            //
            // 1. we are entirely caught up to the writer
            // 2. we've hit the end of the data file and need to go to the next one
            //
            // When we're in this state, we first "wait" for the writer to wake us up.  This might
            // be an existing buffered wakeup, or we might actually be waiting for the next wakeup.
            // Regardless of which type of wakeup it is, we still end up checking if the reader and
            // writer file IDs that we loaded originally match.
            //
            // If the file IDs were identical, it would imply that reader is still on the writer's
            // current data file.  We simply continue the loop in this case.  It may lead to the
            // same thing, `try_read_record` returning `None` with an identical reader/writer file
            // ID, but that's OK, because it would mean we were actually waiting for the writer to
            // make progress now.  If the wakeup was valid, due to writer progress, then, well...
            // we'd actually be able to read data.
            //
            // If the file IDs were not identical, we now know the writer has moved on.  Crucially,
            // since we always flush our writes before waking up, including before moving to a new
            // file, then we know that if the reader/writer were not identical at the start the
            // loop, and `try_read_record` returned `None`, that we have hit the actual end of the
            // reader's current data file, and need to move on.
            self.ledger.wait_for_writer().await;

            if current_writer_file_id != current_reader_file_id {
                let _ = self.roll_to_next_data_file().await.context(Io)?;
            }
        };

        // We got a read token, so our record is present in the reader, and now we can actually read
        // it out and return a reference to it.
        self.update_reader_last_record_id(token.record_id());
        self.track_read(token.record_size() as u64);
        let reader = self
            .reader
            .as_mut()
            .expect("reader should exist after ensure_ready_for_read");
        reader.read_record(token).await
    }
}
