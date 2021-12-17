use std::{
    cmp, fmt,
    io::{self, ErrorKind},
    marker::PhantomData,
    path::PathBuf,
    sync::Arc,
};

use core_common::internal_event::emit;
use crc32fast::Hasher;
use rkyv::{archived_root, AlignedVec};
use snafu::{ResultExt, Snafu};
use tokio::{
    fs::{self, File},
    io::{AsyncBufReadExt, AsyncRead, BufReader},
};

use super::{
    common::create_crc32c_hasher,
    ledger::Ledger,
    record::{try_as_record_archive, Record, RecordStatus},
};
use crate::{encoding::DecodeBytes, internal_events::EventsCorrupted, Bufferable};

#[derive(Debug)]
struct DeletionMarker {
    highest_record_id: u64,
    last_acked_record_id: u64,
    data_file_path: PathBuf,
    bytes_read: u64,
}

pub(super) struct ReadToken(usize, u64);

impl ReadToken {
    pub fn record_size(&self) -> usize {
        self.0
    }

    pub fn record_id(&self) -> u64 {
        self.1
    }

    fn into_id(self) -> u64 {
        self.1
    }
}

/// Error that occurred during calls to [`Reader`].
#[derive(Debug, Snafu)]
pub enum ReaderError<T>
where
    T: Bufferable,
{
    /// A general I/O error occurred.
    ///
    /// Different methods will capture specific I/O errors depending on the situation, as some
    /// errors may be expected and considered normal by design.  For all I/O errors that are
    /// considered atypical, they will be returned as this variant.
    #[snafu(display("read I/O error: {}", source))]
    Io { source: io::Error },

    /// The reader failed to deserialize the record.
    ///
    /// In most cases, this indicates that the data file being read was corrupted or truncated in
    /// some fashion.  Callers of [`Reader::next`] will not actually receive this error, as it is
    /// handled internally by moving to the next data file, as corruption may have affected other
    /// records in a way that is not easily detectable and could lead to records which
    /// deserialize/decode but contain invalid data.
    #[snafu(display("failed to deserialize encoded record from buffer: {}", reason))]
    FailedToDeserialize { reason: String },

    /// The record's checksum did not match.
    ///
    /// In most cases, this indicates that the data file being read was corrupted or truncated in
    /// some fashion.  Callers of [`Reader::next`] will not actually receive this error, as it is
    /// handled internally by moving to the next data file, as corruption may have affected other
    /// records in a way that is not easily detectable and could lead to records which
    /// deserialize/decode but contain invalid data.
    #[snafu(display(
        "calculated checksum did not match the actual checksum: ({} vs {})",
        calculated,
        actual
    ))]
    InvalidChecksum { calculated: u32, actual: u32 },

    /// The decoder encountered an issue during decoding.
    ///
    /// At this stage, the record can be assumed to have been written correctly, and read correctly
    /// from disk, as the checksum was also validated.
    #[snafu(display("failed to decoded record: {:?}", source))]
    FailedToDecode {
        source: <T as DecodeBytes<T>>::Error,
    },
}

impl<T> ReaderError<T>
where
    T: Bufferable,
{
    fn is_bad_read(&self) -> bool {
        matches!(
            self,
            ReaderError::InvalidChecksum { .. } | ReaderError::FailedToDeserialize { .. }
        )
    }
}

/// Buffered reader that handles deserialization, checksumming, and decoding of records.
#[derive(Debug)]
pub(super) struct RecordReader<R, T> {
    reader: BufReader<R>,
    aligned_buf: AlignedVec,
    checksummer: Hasher,
    current_record_id: u64,
    _t: PhantomData<T>,
}

impl<R, T> RecordReader<R, T>
where
    R: AsyncRead + Unpin + fmt::Debug,
    T: Bufferable,
{
    /// Creates a new [`RecordReader`] around the provided reader.
    ///
    /// Internally, the reader is wrapped in a [`BufReader`], so callers should not pass in an
    /// already buffered reader.
    pub fn new(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader),
            aligned_buf: AlignedVec::new(),
            checksummer: create_crc32c_hasher(),
            current_record_id: 0,
            _t: PhantomData,
        }
    }

    #[cfg_attr(test, instrument(skip(self), level = "trace"))]
    async fn read_length_delimiter(&mut self) -> Result<Option<usize>, ReaderError<T>> {
        loop {
            if self.reader.buffer().len() >= 8 {
                let length_buf = &self.reader.buffer()[..8];
                let length = length_buf
                    .try_into()
                    .expect("the slice is the length of a u64");
                self.reader.consume(8);

                // By default, records cannot exceed 8MB in length, so whether our `usize` is a u32
                // or u64, we're not going to overflow it.  While the maximum record size _can_ be
                // changed, it's not currently exposed to users.  Even further, if it was exposed to
                // users, it's currently a `usize`, so again, we know that we're not going to exceed
                // 64-bit. And even further still, the writer fallibly attempts to get a `u64` of the
                // record size based on the encoding buffer, which gives its length in `usize`, and
                // so would fail if `usize` was larger than `u64`, meaning we at least will panic if
                // Vector is running on a 128-bit CPU in the future, storing records that are larger
                // than 2^64+1. :)
                let record_len = u64::from_be_bytes(length)
                    .try_into()
                    .expect("record length should never exceed usize");
                return Ok(Some(record_len));
            }

            let buf = self.reader.fill_buf().await.context(Io)?;
            if buf.is_empty() {
                return Ok(None);
            }
        }
    }

    /// Attempts to read a record.
    ///
    /// In order to facilitate driving other logic within the reader when there is no data to
    /// currently read, this method returns early when there is no available data at all, rather
    /// than waiting for enough data to start readiong a normal record.  In cases where there is not
    /// enough data to begin reading a record, `None` is returned.
    ///
    /// If there is any available data, even if it's not _enough_ data, then this method will
    /// continue awaiting until it can read an entire record.
    ///
    /// If a record is able to be read in its entirety, a token is returned to caller that can be
    /// used with [`read_record`] in order to get an owned `T`.  This is due to a quirk with the
    /// compiler's ability to track stacked mutable references through conditional control flows, of
    /// which is handled by splitting the "do we have a valid record in our buffer?" logic from the
    /// "read that record and decode it" logic.
    ///
    /// # Errors
    ///
    /// Errors can occur during the I/O or deserialization stage.  If an error occurs during any of
    /// these stages, an appropriate error variant will be returned describing the error.
    #[cfg_attr(test, instrument(skip(self), level = "trace"))]
    pub async fn try_next_record(&mut self) -> Result<Option<ReadToken>, ReaderError<T>> {
        let record_len = if let Some(len) = self.read_length_delimiter().await? {
            len
        } else {
            trace!("read_length_delimiter returned None");
            return Ok(None);
        };

        if record_len == 0 {
            return Err(ReaderError::FailedToDeserialize {
                reason: "record length was zero".to_string(),
            });
        }

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
            RecordStatus::FailedDeserialization(de) => Err(ReaderError::FailedToDeserialize {
                reason: de.into_inner(),
            }),
            RecordStatus::Corrupted { calculated, actual } => {
                Err(ReaderError::InvalidChecksum { calculated, actual })
            }
            RecordStatus::Valid(id) => {
                self.current_record_id = id;
                // TODO: Another spot where our hardcoding of the length delimiter size in bytes is fragile.
                Ok(Some(ReadToken(8 + buf.len(), id)))
            }
        }
    }

    /// Reads the record associated with the given [`ReadToken`].
    ///
    /// # Errors
    ///
    /// If an error occurs during decoding, an error variant will be returned describing the error.
    ///
    /// # Panics
    ///
    /// If a `ReadToken` is not used in a call to `read_record` before again calling
    /// `try_next_record`, and the `ReadToken` from _that_ call is used, this method will panic due
    /// to an out-of-order read.
    pub fn read_record(&mut self, token: ReadToken) -> Result<T, ReaderError<T>> {
        let record_id = token.into_id();
        if record_id != self.current_record_id {
            panic!("using expired read token");
        }

        // SAFETY:
        // - `try_next_record` is the only method that can hand back a `ReadToken`
        // - we only get a `ReadToken` if there's a valid record in `self.aligned_buf`
        // - `try_next_record` does all the archive checks, checksum validation, etc
        let archived_record = unsafe { archived_root::<Record<'_>>(&self.aligned_buf) };

        T::decode(archived_record.payload()).context(FailedToDecode)
    }
}

/// Reads records from the buffer.
#[derive(Debug)]
pub struct Reader<T> {
    ledger: Arc<Ledger>,
    reader: Option<RecordReader<File, T>>,
    bytes_read: u64,
    last_reader_record_id: u64,
    last_acked_record_id: u64,
    ready_to_read: bool,
    check_pending_deletions: bool,
    pending_deletions: Vec<DeletionMarker>,
    pending_read_sizes: Vec<u64>,
    _t: PhantomData<T>,
}

impl<T> Reader<T>
where
    T: Bufferable,
{
    /// Creates a new [`Reader`] attached to the given [`Ledger`].
    pub(crate) fn new(ledger: Arc<Ledger>) -> Self {
        Reader {
            ledger,
            reader: None,
            bytes_read: 0,
            last_reader_record_id: 0,
            last_acked_record_id: 0,
            ready_to_read: false,
            check_pending_deletions: false,
            pending_deletions: Vec::new(),
            pending_read_sizes: Vec::new(),
            _t: PhantomData,
        }
    }

    fn track_read(&mut self, record_size: u64) {
        // Tracking reads involves two main aspects:
        // - keeping track of the bytes we've read _in this current data file_
        // - keeping track of the total buffer size overall
        //
        // When we roll over to the next data file, we need to check to see if we read the entire
        // data file.  In some case, such as hitting a corrupted record, we'll skip the rest of the
        // file.  Keeping track of all the bytes we read for the given data file allows us to
        // compensate for skipped data file scenarios.
        //
        // After that, we need to make sure we're correctly updating the ledger.  If we're still
        // initializing our reader (aka `seek_to_next_record` has not completed yet) then we need to
        // directly update the ledger as we read, to ensure that the resulting "total buffer size"
        // is accurate once the both the reader and writer have been created.  Otherwise, we only
        // update the ledger once reads are acknowledged, which requires storing a pending read size.
        self.bytes_read += record_size;

        if self.ready_to_read {
            self.pending_read_sizes.push(record_size);
        } else {
            self.ledger.decrement_total_buffer_size(record_size);
        }
    }

    #[cfg_attr(test, instrument(skip_all, level = "debug"))]
    async fn delete_completed_data_file(&mut self, marker: DeletionMarker) -> io::Result<()> {
        debug!(message = "deleting completed data file", ?marker);

        // Grab the size of the data file before we delete it, which gives us a chance to fix up the
        // total buffer size for corrupted files.
        //
        // Since we only decrement the buffer size after a successful read in normal cases, skipping
        // the rest of a corrupted file could lead to the total buffer size being unsynchronized.
        // We use the difference between the number of bytes read and the file size to figure out if
        // we need to make a manual adjustment.
        let data_file = File::open(&marker.data_file_path).await?;
        let metadata = data_file.metadata().await?;

        let size_delta = metadata.len() - marker.bytes_read;
        if size_delta > 0 {
            debug!(
                "fixing up buffer size after deleting partially-read data file: delta={} ({}-{})",
                size_delta,
                metadata.len(),
                marker.bytes_read
            );
            self.ledger.decrement_total_buffer_size(size_delta);
        }
        drop(data_file);

        // Delete the current data file, and increment our actual reader file ID.
        fs::remove_file(&marker.data_file_path).await?;
        self.ledger.increment_acked_reader_file_id();
        self.ledger.flush()?;

        debug!("flushed after deleting data file, notifying writers and continuing");

        // Notify any waiting writers that we've deleted a data file, which they may be waiting on
        // because they're looking to reuse the file ID of the file we just finished reading.
        self.ledger.notify_reader_waiters();

        Ok(())
    }

    #[cfg_attr(test, instrument(skip(self), level = "debug"))]
    async fn delete_completed_data_files(&mut self) -> io::Result<()> {
        // Figure out if any of the pending deletions we have are now ready.
        let ready_deletions = {
            let ready_deletions_len = self
                .pending_deletions
                .iter()
                .take_while(|pending| {
                    // If we haven't wrapped around past zero, then we simply check if our new "last acked"
                    // value is highest than the highest record ID for the deletion, otherwise we check if
                    // the number of acknowledged records exceeds the difference between "last acked" and
                    // "highest record ID", which handles the wrapping-to-zero case.
                    self.last_acked_record_id >= pending.highest_record_id
                        || (self.last_acked_record_id <= pending.last_acked_record_id
                            && pending.highest_record_id > pending.last_acked_record_id)
                })
                .count();
            self.pending_deletions
                .drain(..ready_deletions_len)
                .collect::<Vec<_>>()
        };

        // Delete each data file whose maximum record ID we've now exceeded.
        for deletion in ready_deletions {
            self.delete_completed_data_file(deletion).await?;
        }

        Ok(())
    }

    #[cfg_attr(test, instrument(skip(self), level = "debug"))]
    async fn adjust_acknowledgement_state(&mut self, ack_offset: u64) -> io::Result<()> {
        // Track our new highest acknowledged record ID, and handle any pending deletions that now qualify.
        self.last_acked_record_id = self.last_acked_record_id.wrapping_add(ack_offset);
        self.ledger
            .state()
            .increment_last_reader_record_id(ack_offset);
        self.delete_completed_data_files().await
    }

    #[cfg_attr(test, instrument(skip(self), level = "debug"))]
    async fn handle_pending_acknowledgements(&mut self) -> io::Result<()> {
        // In this method, we handle ensuring that whatever pending deletions that are now "ready"
        // are handled as quickly as possible.
        //
        // Normally, as records are read, they'll be processed downstream, which can take a variable
        // amount of time.  Due to this, an object is provided to callers which they use to indicate
        // how many of the records they've read have been durably processed such that we can
        // consider them processed from the reader perspective.  In other words, they "acknowledge"
        // that we no longer need to care about those records.
        //
        // When all of the records for a given data file have been acknowledged, this means we can
        // finally delete that data file.

        // Consume however many outstanding pending acknowledgements there are and handling
        // adjusting the ledger, as well as seeing if any pending deletions are now ready to run.
        let pending_acks = self.ledger.consume_pending_acks();
        if pending_acks > 0 {
            // First, recognize all of the bytes read for the now-acknowledged records so we can
            // adjust the total buffer size correctly.
            let total_bytes_read = self.pending_read_sizes.drain(..pending_acks).sum();
            let ack_count = pending_acks
                .try_into()
                .expect("number of pending acks should always fit into u64");

            self.ledger.track_reads(ack_count, total_bytes_read);

            // Notify any waiting writers that we've consumed a bunch of records/bytes, since they
            // might be waiting for the total buffer size to go down below the configured limit.
            self.ledger.notify_reader_waiters();

            self.adjust_acknowledgement_state(pending_acks as u64)
                .await?;
        }

        // Due to the structure of `next`, we handle pending acknowledgements before we generate
        // deletion markers.  This poses a problem when `next` handles any pending acknowledgements and
        // then also rolls to the next data file all in the same call.  Since the pending deletion
        // isn't generated until after calling this method, we end up in a situation where
        // `pending_acks` is zero, even though the deletion marker we just generated is ready.
        //
        // To handle this case, all file rolling operations set the `self.pending_acks_checked` flag
        // to indicate that a pending deletion was recently inserted and that we should check for
        // any pending deletions that are ready, even if we had no pending acknowledgements.
        let result = if self.check_pending_deletions {
            self.check_pending_deletions = false;
            self.delete_completed_data_files().await
        } else {
            Ok(())
        };

        if self.ready_to_read {
            trace!(
                "finished handling acknowledgements, total buffer size = {}",
                self.ledger.get_total_buffer_size()
            );
        }
        result
    }

    /// Switches the reader over to the next data file to read.
    #[cfg_attr(test, instrument(skip(self), level = "debug"))]
    fn roll_to_next_data_file(&mut self) {
        // Store the pending deletion marker.
        let marker = DeletionMarker {
            highest_record_id: self.last_reader_record_id,
            last_acked_record_id: self.last_acked_record_id,
            data_file_path: self.ledger.get_current_reader_data_file_path(),
            bytes_read: self.bytes_read,
        };
        debug!(message = "marking data file for deletion", ?marker);
        self.pending_deletions.push(marker);
        self.check_pending_deletions = true;

        // Now reset our internal state so we can go for the next data file.
        self.reader = None;
        self.bytes_read = 0;
        self.ledger.increment_unacked_reader_file_id();
    }

    /// Ensures this reader is ready to attempt reading the next record.
    #[cfg_attr(test, instrument(skip(self), level = "debug"))]
    async fn ensure_ready_for_read(&mut self) -> io::Result<()> {
        // We have nothing to do if we already have a data file open.
        if self.reader.is_some() {
            return Ok(());
        }

        // Try to open the current reader data file.  This might not _yet_ exist, in which case
        // we'll simply wait for the writer to signal to us that progress has been made, which
        // implies a data file existing.
        loop {
            let (reader_file_id, writer_file_id) = self.ledger.get_current_reader_writer_file_id();
            let data_file_path = self.ledger.get_current_reader_data_file_path();
            let data_file = match File::open(&data_file_path).await {
                Ok(data_file) => data_file,
                Err(e) => match e.kind() {
                    ErrorKind::NotFound => {
                        // TODO: Add a test that can correctly suss out this situation and prove
                        // this invariant for us:
                        //
                        // If the reader/writer file IDs are not the same, and we try to open our
                        // current file and it doesn't exist, we're in a scenario where we deleted
                        // the file but didn't get to the step where we increment our "current
                        // reader file ID".  We simply increment that to set ourselves to the right
                        // position and try again.
                        if reader_file_id == writer_file_id {
                            debug!("waiting for {:?} to be created...", data_file_path);
                            self.ledger.wait_for_writer().await;
                            debug!("notified by writer, trying to load the data file again");
                        } else {
                            self.ledger.increment_acked_reader_file_id();
                        }
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

    /// Updates the internal read progress state, and potentially deletes completed data files.
    ///
    /// # Errors
    ///
    /// As this method may potentially drive the "delete completed data files" logic, an I/O error
    /// could be encountered during that phase.  If an I/O error _is_ encountered, an error variant
    /// will be returned describing the error.
    #[cfg_attr(test, instrument(skip(self), level = "debug"))]
    async fn update_reader_last_record_id(&mut self, record_id: u64) -> io::Result<()> {
        let previous_id = self.last_reader_record_id;
        self.last_reader_record_id = record_id;

        // Don't execute the ID delta logic when we're still in setup mode, which is where we would
        // be reading record IDs below our last read record ID.
        if !self.ready_to_read {
            return Ok(());
        }

        // Figure out the delta between the last ID we marked ourselves as having read, and this one.
        let id_delta = record_id.wrapping_sub(previous_id);
        assert!(
            id_delta != 0,
            "record IDs are monotonic, detected identical record ID read"
        );

        // When records are being read correctly, each delta should be equal to 1, so there's
        // nothing for us to do there.  If there was a bug, or if we skipped over corrupted records,
        // we would see the delta be greater than one.  This means we need to actually report that
        // this occurred, and adjust our internal acknowledgement state.
        //
        // If we didn't acknowledge the fact that we just skipped N records, our acknowledgement
        // code would end up N behind, leading to an internal inconsistency.  Skipping the file _is_
        // tantamount to acknowledging the skipped records, because we're saying they're corrupted
        // and can't be trusted.  We don't ever want to try re-reading them again.
        if id_delta > 1 {
            let corrupted_records = id_delta - 1;
            debug!(
                "detected {} missing records ({} -> {}), adjusting...",
                corrupted_records, previous_id, record_id
            );
            emit(&EventsCorrupted {
                count: corrupted_records,
            });

            // We call this here, instead of incrementing `pending_acks` in the ledger
            // directly, or waiting for the next call to `next`, for two reasons:
            // - we can delete data files faster, potentially, by doing it right before
            //   `next` successfully returns a record
            // - since we're skipping records, there's no pending read sizes for the records
            //   we're skipping, so we just need to update the acknowledgement state
            //   directly, without going through the normal path
            //
            // We maintain ledger consistency, even without going through
            // `handle_pending_acknowledgements`, by updating the record count above, and
            // the data file deletion logic handles fixing up the buffer size.
            self.adjust_acknowledgement_state(corrupted_records).await?;
        }

        Ok(())
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
    ///
    /// # Errors
    ///
    /// If an error occurs during seeking to the next record, an error variant will be returned
    /// describing the error.
    #[cfg_attr(test, instrument(skip(self), level = "debug"))]
    pub(super) async fn seek_to_next_record(&mut self) -> Result<(), ReaderError<T>> {
        debug!("seeking to the last record acknowledged for this reader");

        // We don't try seeking again once we're all caught up.
        if self.ready_to_read {
            warn!("reader already seeked, skipping seek_to_next_record");
            return Ok(());
        }

        // We rely on `next` to close out the data file if we've actually reached the end, and we
        // also rely on it to reset the data file before trying to read, and we _also_ rely on it to
        // update `self.last_reader_record_id`, so basically... just keep reading records until we
        // get to the one we left off with last time.
        let starting_self_last = self.last_reader_record_id;
        let ledger_last = self.ledger.state().get_last_reader_record_id();
        debug!(
            "currentl at {}, seeking to {} (per ledger)",
            self.last_reader_record_id, ledger_last
        );

        while self.last_reader_record_id < ledger_last {
            if self.next().await?.is_none() && starting_self_last == self.last_reader_record_id {
                // The reader told us that they've hit the end of whatever file they're current on.
                // If `self.last_reader_record_id` hasn't moved at all, compared to when we started
                // (starting_last_reader_record_id), then we know that we're caught up, and we just
                // need to set `self.last_reader_record_id` to match what the ledger has.
                self.update_reader_last_record_id(ledger_last)
                    .await
                    .context(Io)?;
                break;
            }
        }

        self.last_acked_record_id = ledger_last;

        debug!("seeked to {} without issue, reader ready", ledger_last);

        self.ready_to_read = true;

        Ok(())
    }

    /// Reads a record.
    ///
    /// If the writer is closed and there is no more data in the buffer, `None` is returned.
    /// Otherwise, reads the next record or waits until the next record is available.
    ///
    /// # Errors
    ///
    /// If an error occurred while reading a record, an error variant will be returned describing
    /// the error.
    #[cfg_attr(test, instrument(skip(self), level = "trace"))]
    pub async fn next(&mut self) -> Result<Option<T>, ReaderError<T>> {
        let token = loop {
            // Handle any pending acknowledgements first.
            self.handle_pending_acknowledgements().await.context(Io)?;

            self.ensure_ready_for_read().await.context(Io)?;

            let reader = self
                .reader
                .as_mut()
                .expect("reader should exist after `ensure_ready_for_read`");

            let (reader_file_id, writer_file_id) = self.ledger.get_current_reader_writer_file_id();

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
                // TODO: Explore the concept of putting a data file into a "one more attempt to read
                // a valid record" state, almost like a semi-open circuit breaker.  There's a
                // possibility that the length delimiter we got is valid, and all the data was
                // written for the record, but the data was invalid... and that if we just kept
                // reading, we might actually encounter a valid record.
                //
                // Theoretically, based on both the validation done by `rkyv` and the checksum, it
                // should be incredibly incredibly unlikely to read a valid record after getting a
                // corrupted record if there was missing data or more invalid data.  We use
                // checksumming to assert errors within a given chunk of the payload, so one payload
                // being corrupted doesn't always, in fact, mean that other records after it are
                // corrupted too.
                Err(e) => {
                    // Invalid checksums and deserialization failures can't really be acted upon by
                    // the caller, but they might be expecting a read-after-write behavior, so we
                    // return the error to them after ensuring that we roll to the next file first.
                    if e.is_bad_read() {
                        self.roll_to_next_data_file();
                    }

                    return Err(e);
                }
            };

            // Fundamentally, when `try_read_record` returns `None`, there's three possible
            // scenarios:
            //
            // 1. we are entirely caught up to the writer
            // 2. we've hit the end of the data file and need to go to the next one
            // 3. the writer has closed/dropped/finished/etc
            //
            // When we're at this point, we first "wait" for the writer to wake us up.  This might
            // be an existing buffered wake-up, or we might actually be waiting for the next
            // wake-up.  Regardless of which type of wakeup it is, we wait for a wake up.  The
            // writer will always issue a wake-up when it finishes any major operation: creating a
            // new data file, flushing, closing, etc.
            //
            // After that, we check to see if the writer is done: this means the writer has
            // explicitly closed itself and will send no more messages to this specific
            // reader/writer pair.  We only return `None` ourselves if we've also drained all
            // remaining records from the buffer.
            //
            // After that, we check the reader/writer file IDs.  If the file IDs were identical, it
            // would imply that reader is still on the writer's current data file.  We simply
            // continue the loop in this case.  It may lead to the same thing --`try_read_record`
            // returning `None` with an identical reader/writer file ID -- but that's OK, because it
            // would mean we were actually waiting for the writer to make progress now.  If the
            // wake-up was valid, due to writer progress, then, well...  we'd actually be able to
            // read data.
            //
            // If the file IDs were not identical, we now know the writer has moved on.  Crucially,
            // since we always flush our writes before waking up, including before moving to a new
            // file, then we know that if the reader/writer were not identical at the start the
            // loop, and `try_read_record` returned `None`, that we have hit the actual end of the
            // reader's current data file, and need to move on.
            if self.ready_to_read {
                self.ledger.wait_for_writer().await;

                if self.ledger.is_writer_done() && self.ledger.get_total_buffer_size() == 0 {
                    // NOTE: We specifically check the total buffer size as it gets updated sooner -- in
                    // `roll_to_next_data_file` -- versus total records, which needs a successful read
                    // to catch any inconsistencies in the record IDs.
                    //
                    // This means that if we encountered a corrupted record as the last record we had to
                    // read before the above if condition would be met, our `next` call would hit the
                    // corrupted record, detect that, roll to the next file, which would do the buffer
                    // size adjustments, and then the following call to `next` would fallthrough to
                    // here.
                    //
                    // The same scenario with total records would be stuck waiting as we would have no
                    // more records to read to drive the check that fixes total records when we detect
                    // skipping record IDs.
                    return Ok(None);
                }
            } else {
                // We're currently just seeking to where we left off the last time this buffer was
                // running, which might mean there's no records for us to read at all because we
                // were already caught up.  All we can do is signal to `seek_to_next_record` that
                // we're caught up.
                return Ok(None);
            }

            if reader_file_id != writer_file_id {
                debug!(
                    "file read had no data, reader/writer file IDs at {} and {}, rolling",
                    reader_file_id, writer_file_id
                );
                self.roll_to_next_data_file();
            }
        };

        // We got a read token, so our record is present in the reader, and now we can actually read
        // it out and return a reference to it.
        if self.ready_to_read {
            trace!(
                "read record ID {} with total size {}",
                token.record_id(),
                token.record_size()
            );
        }

        self.update_reader_last_record_id(token.record_id())
            .await
            .context(Io)?;
        self.track_read(token.record_size() as u64);
        let reader = self
            .reader
            .as_mut()
            .expect("reader should exist after `ensure_ready_for_read`");
        reader.read_record(token).map(Some)
    }
}

impl<T> Drop for Reader<T> {
    fn drop(&mut self) {
        debug!("ledger state at reader drop: {:#?}", self.ledger.state());
    }
}
