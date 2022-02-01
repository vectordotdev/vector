use std::{
    cmp, fmt,
    io::{self, ErrorKind},
    marker::PhantomData,
    path::PathBuf,
    sync::Arc,
};

use crc32fast::Hasher;
use rkyv::{archived_root, AlignedVec};
use snafu::{ResultExt, Snafu};
use tokio::{
    fs::{self, File},
    io::{AsyncBufReadExt, AsyncRead, BufReader},
};
use vector_common::internal_event::emit;

use super::{
    common::create_crc32c_hasher,
    ledger::Ledger,
    record::{try_as_record_archive, Record, RecordStatus},
};
use crate::{
    encoding::{AsMetadata, Encodable},
    internal_events::EventsCorrupted,
    Bufferable,
};

#[derive(Debug)]
struct DeletionMarker {
    highest_record_id: u64,
    last_acked_record_id: u64,
    data_file_path: PathBuf,
    bytes_read: u64,
}

#[derive(Debug)]
struct DelayedAck {
    record_id_threshold: u64,
    amount: u64,
}

pub(super) struct ReadToken {
    record_id: u64,
    record_len: usize,
}

impl ReadToken {
    pub fn new(record_id: u64, record_len: usize) -> Self {
        Self {
            record_id,
            record_len,
        }
    }

    pub fn record_id(&self) -> u64 {
        self.record_id
    }

    pub fn record_len(&self) -> usize {
        self.record_len
    }

    fn into_record_id(self) -> u64 {
        self.record_id
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
    Deserialization { reason: String },

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
    Checksum { calculated: u32, actual: u32 },

    /// The decoder encountered an issue during decoding.
    ///
    /// At this stage, the record can be assumed to have been written correctly, and read correctly
    /// from disk, as the checksum was also validated.
    #[snafu(display("failed to decoded record: {:?}", source))]
    Decode {
        source: <T as Encodable>::DecodeError,
    },

    /// The record is not compatible with this version of Vector.
    ///
    /// This can occur when records written to a buffer in previous versions of Vector are read by
    /// newer versions of Vector where the encoding scheme, or record schema, used in the previous
    /// version of Vector are no longer able to be decoded in this version of Vector.
    #[snafu(display("record version not compatible: {}", reason))]
    Incompatible { reason: String },

    /// The reader detected that a data file contains a partially-written record.
    ///
    /// Records should never be partially written to a data file (we don't split records across data
    /// files) so this would be indicative of a write that was never properly written/flushed, or
    /// some issue with the write where it was acknowledged but the data/file was corrupted in same way.
    ///
    /// This is effectively the same class of error as an invalid checksum/failed deserialization.
    PartialWrite,
}

impl<T> ReaderError<T>
where
    T: Bufferable,
{
    fn is_bad_read(&self) -> bool {
        matches!(
            self,
            ReaderError::Checksum { .. }
                | ReaderError::Deserialization { .. }
                | ReaderError::PartialWrite
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
    async fn read_length_delimiter(
        &mut self,
        is_finalized: bool,
    ) -> Result<Option<usize>, ReaderError<T>> {
        loop {
            let available = self.reader.buffer().len();
            if available >= 8 {
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

            // We don't have enough bytes, so we need to fill our buffer again.
            let buf = self.reader.fill_buf().await.context(IoSnafu)?;
            if buf.is_empty() {
                return Ok(None);
            }

            // If we tried to read more bytes, and we still don't have enough for the record
            // delimiter, and the data file has been finalized already: we've got a partial
            // write situation on our hands.
            if buf.len() < 8 && is_finalized {
                return Err(ReaderError::PartialWrite);
            }
        }
    }

    /// Attempts to read a record.
    ///
    /// Records are prececded by a length delimiter, a fixed-size integer (currently 8 bytes) that
    /// tells the reader how many more bytes to read in order to completely read the next record.
    ///
    /// If there are no more bytes to read, we return early in order to allow the caller to wait
    /// until such a time where there should be more data, as no wake-ups can be generated when
    /// reading a file after reaching EOF.
    ///
    /// If there is any data available, we attempt to continue reading until both a length
    /// delimiter, and the accompanying record, can be read in their entirety.
    ///
    /// If a record is able to be read in its entirety, a token is returned to caller that can be
    /// used with [`read_record`] in order to get an owned `T`.  This is due to a quirk with the
    /// compiler's ability to track stacked mutable references through conditional control flows, of
    /// which is handled by splitting the "do we have a valid record in our buffer?" logic from the
    /// "read that record and decode it" logic.
    ///
    /// # Finalized reads
    ///
    /// All of the above logic applies when `is_finalized` is `false`, which signals that a data
    /// file is still currently being written to.  If `is_finalized` is `true`, most of the above
    /// logic applies but in cases where we detect a partial write, we explicitly return an error
    /// for a partial read.
    ///
    /// In practice, what this means is that when we believe a file should be "finalized" -- the
    /// writer flushed the file to disk, the ledger has been flushed, etc -- then we also expect to
    /// be able to read all bytes with no leftover.  A partially-written length delimiter, or
    /// record, would be indicative of a bug with the writer or OS/disks, essentially telling us
    /// that the current data file is not valid for reads anymore.  We don't know _why_ it's in this
    /// state, only that something is not right and that we must skip the file.
    ///
    /// # Errors
    ///
    /// Errors can occur during the I/O or deserialization stage.  If an error occurs during any of
    /// these stages, an appropriate error variant will be returned describing the error.
    #[cfg_attr(test, instrument(skip(self), level = "trace"))]
    pub async fn try_next_record(
        &mut self,
        is_finalized: bool,
    ) -> Result<Option<ReadToken>, ReaderError<T>> {
        let record_len = if let Some(len) = self.read_length_delimiter(is_finalized).await? {
            len
        } else {
            trace!("read_length_delimiter returned None");
            return Ok(None);
        };

        if record_len == 0 {
            return Err(ReaderError::Deserialization {
                reason: "record length was zero".to_string(),
            });
        }

        // Read in all of the bytes we need first.
        self.aligned_buf.clear();
        while self.aligned_buf.len() < record_len {
            let needed = record_len - self.aligned_buf.len();
            let buf = self.reader.fill_buf().await.context(IoSnafu)?;
            if buf.is_empty() && is_finalized {
                // If we needed more data, but there was none available, and we're finalized: we've
                // got ourselves a partial write situation.
                return Err(ReaderError::PartialWrite);
            }

            let available = cmp::min(buf.len(), needed);
            self.aligned_buf.extend_from_slice(&buf[..available]);
            self.reader.consume(available);
        }

        // Now see if we can deserialize our archived record from this.
        let buf = self.aligned_buf.as_slice();
        match try_as_record_archive(buf, &self.checksummer) {
            RecordStatus::FailedDeserialization(de) => Err(ReaderError::Deserialization {
                reason: de.into_inner(),
            }),
            RecordStatus::Corrupted { calculated, actual } => {
                Err(ReaderError::Checksum { calculated, actual })
            }
            RecordStatus::Valid { id, .. } => {
                self.current_record_id = id;
                // TODO: Another spot where our hardcoding of the length delimiter size in bytes is fragile.
                Ok(Some(ReadToken::new(id, 8 + buf.len())))
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
        let record_id = token.into_record_id();
        assert_eq!(
            self.current_record_id, record_id,
            "using expired read token; this is a serious bug"
        );

        // SAFETY:
        // - `try_next_record` is the only method that can hand back a `ReadToken`
        // - we only get a `ReadToken` if there's a valid record in `self.aligned_buf`
        // - `try_next_record` does all the archive checks, checksum validation, etc
        let record = unsafe { archived_root::<Record<'_>>(&self.aligned_buf) };

        // Try and convert the raw record metadata into the true metadata type used by `T`, and then
        // also verify that `T` is able to decode records with the metadata used for this record in particular.
        let metadata =
            T::Metadata::from_u32(record.metadata()).ok_or(ReaderError::Incompatible {
                reason: format!("invalid metadata for {}", std::any::type_name::<T>()),
            })?;

        if !T::can_decode(metadata) {
            return Err(ReaderError::Incompatible {
                reason: format!(
                    "record metadata not supported (metadata: {:#036b})",
                    record.metadata()
                ),
            });
        }

        // Now we can finally try decoding.
        T::decode(metadata, record.payload()).context(DecodeSnafu)
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
    delayed_acks: Vec<DelayedAck>,
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
            delayed_acks: Vec::new(),
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
        trace!(
            message = "scanning for pending data files now eligible for deletion",
            self.last_acked_record_id
        );

        // Figure out if any of the pending deletions we have are now ready.
        let ready_deletions = {
            let ready_deletions_len = self
                .pending_deletions
                .iter()
                .take_while(|pending| {
                    trace!(message = "examining pending deletion", marker = ?pending);
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
        // Calculate our new `last_acked_record_id`, and then proactively drain any delayed
        // acknowledgements that would become eligible by either adding `ack_offset`, or any
        // knock-on acknowledgement changes from an eligible delayed acknowledgement.
        //
        // For example, if we had last_acked_record_id=2, and two delayed acknowledgements -- one
        // with a threshold of 4 and one with a threshold of 6, both with an amount of 2, then our
        // outstanding expected acknowledgement timeline would look like:
        //
        //                           [ delayed ack region 4/2 ][ delayed ack region 6/2 ]
        // [record ID 3][record ID 4][record ID 5][record ID 6][record ID 7][record ID 8]
        //
        // This means that if we acknowledge record IDs 3 and 4 (ack_offset=2), we would make the
        // first delayed acknowledgement eligible, because `last_acked_record_id` would now be 4.
        // However, once we also incorporated the acknowledgements from the first delayed ack, then
        // the _second_ delayed acknowledgement would become eligible, since `last_acked_record_id`
        // would now be at 6.
        //
        // Thus, we track the estimated `last_acked_record_id` for every delayed acknowledgement
        // that becomes eligible so that we can maximally consume them in this one call.
        let mut new_last_acked_record_id = self.last_acked_record_id.wrapping_add(ack_offset);
        while !self.delayed_acks.is_empty() {
            let delayed_ack = self
                .delayed_acks
                .first()
                .expect("delayed_acks cannot be empty");
            if new_last_acked_record_id >= delayed_ack.record_id_threshold {
                let delayed_ack = self.delayed_acks.remove(0);
                new_last_acked_record_id =
                    new_last_acked_record_id.wrapping_add(delayed_ack.amount);
            }
        }

        // Now calculate the actual delta to apply to `last_acked_record_id` and make it so.
        let last_acked_delta = new_last_acked_record_id.wrapping_sub(self.last_acked_record_id);
        self.last_acked_record_id = self.last_acked_record_id.wrapping_add(last_acked_delta);
        self.ledger
            .state()
            .increment_last_reader_record_id(last_acked_delta);

        // Pending data file deletions may be eligible now, so check.
        self.delete_completed_data_files().await
    }

    fn add_delayed_ack_range(&mut self, record_id_threshold: u64, amount: u64) {
        self.delayed_acks.push(DelayedAck {
            record_id_threshold,
            amount,
        });
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

            // Adjust our acknowledgement state, which may also trigger delayed acknowledgements
            // to fire depending on how much acknowledgement progress we make from "real"
            // acknowledgements.  We do this before notifying the writer because if we do end up
            // allowing a delayed acknowledgement to proceed, we may free up even more bytes in the
            // buffer.
            self.adjust_acknowledgement_state(pending_acks as u64)
                .await?;

            // Notify any waiting writers that we've consumed a bunch of records/bytes, since they
            // might be waiting for the total buffer size to go down below the configured limit.
            self.ledger.notify_reader_waiters();
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

            debug!("reader opened data file '{:?}'", data_file_path);

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

            // Track the range of record IDs for the corrupted region, which ensures that we won't
            // acknowledge them until all record IDs before the start of the range have also been
            // acknowledged.  This avoids potentially acknowledging not-yet-acknowledged records
            // which are part of a corrupted data file, since it could cause the data file to be
            // deleted before those not-yet-acknowledged records were _actually_ acknowledged.
            //
            // Essentially, we need to wait until the last valid record ID that we read has been
            // acknowledged, and then we can fast-forward acknowledge the record ID range for the
            // corrupted records we just discovered.
            self.add_delayed_ack_range(previous_id, corrupted_records);
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
        debug!("seeking to last acknowledged record for reader");

        // We don't try seeking again once we're all caught up.
        if self.ready_to_read {
            warn!("reader already initialized, skipping");
            return Ok(());
        }

        // We rely on `next` to close out the data file if we've actually reached the end, and we
        // also rely on it to reset the data file before trying to read, and we _also_ rely on it to
        // update `self.last_reader_record_id`, so basically... just keep reading records until we
        // get to the one we left off with last time.
        let starting_self_last = self.last_reader_record_id;
        let ledger_last = self.ledger.state().get_last_reader_record_id();
        debug!(
            "starting from {}, seeking to {} (per ledger)",
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
                    .context(IoSnafu)?;
                break;
            }
        }

        self.last_acked_record_id = ledger_last;

        debug!("seeked to {}, reader ready", ledger_last);

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
            self.handle_pending_acknowledgements()
                .await
                .context(IoSnafu)?;

            // If the writer has marked themselves as done, and the buffer has been emptied, then
            // we're done and can return.  We have to look at something besides simply the writer
            // being marked as done to know if we're actually done or not, and "buffer size" is better
            // than "total records" because we update buffer size when handling acknowledgements,
            // whether it's an individual ack or an entire file being deleted.
            //
            // If we used "total records", we could end up stuck in cases where we skipped
            // corrupted records, but hadn't yet had a "good" record that we could read, since the
            // "we skipped records due to corruption" logic requires performing valid read to
            // detect, and calculate a valid delta from.
            if self.ledger.is_writer_done() {
                let total_buffer_size = self.ledger.get_total_buffer_size();
                if total_buffer_size == 0 {
                    return Ok(None);
                }
            }

            self.ensure_ready_for_read().await.context(IoSnafu)?;

            let reader = self
                .reader
                .as_mut()
                .expect("reader should exist after `ensure_ready_for_read`");

            let (reader_file_id, writer_file_id) = self.ledger.get_current_reader_writer_file_id();

            // Essentially: is the writer still writing to this data file or not?
            //
            // A necessary invariant to have to understand if the record reader should actually keep
            // waiting for data, or if a data file had a partial write/missing data and should be skipped.
            let is_finalized = reader_file_id != writer_file_id;

            // Try reading a record, which if successful, gives us a token to actually read/get a
            // reference to the record.  This is a slightly-tricky song-and-dance due to rustc not
            // yet fully understanding mutable borrows when conditional control flow is involved.
            match reader.try_next_record(is_finalized).await {
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
            // When we're at this point, we "wait" for the writer to wake us up.  This might
            // be an existing buffered wake-up, or we might actually be waiting for the next
            // wake-up.  Regardless of which type of wakeup it is, we wait for a wake up.  The
            // writer will always issue a wake-up when it finishes any major operation: creating a
            // new data file, flushing, closing, etc.
            //
            // After that, we check the reader/writer file IDs.  If the file IDs were identical, it
            // would imply that reader is still on the writer's current data file.  We simply
            // continue the loop in this case.  It may lead to the same thing --`try_read_record`
            // returning `None` with an identical reader/writer file ID -- but that's OK, because it
            // would mean we were actually waiting for the writer to make progress now.  If the
            // wake-up was valid, due to writer progress, then, well... we'd actually be able to
            // read data.
            //
            // If the file IDs were not identical, we now know the writer has moved on.  Crucially,
            // since we always flush our writes before waking up, including before moving to a new
            // file, then we know that if the reader/writer were not identical at the start the
            // loop, and `try_read_record` returned `None`, that we have hit the actual end of the
            // reader's current data file, and need to move on.
            //
            // The case of "the writer has closed/dropped/finished/etc" is handled at the top of the
            // loop, because otherwise we could get stuck waiting for the writer after an empty
            // `try_read_record` attempt when the writer is done and we're at the end of the file,
            // etc.
            if self.ready_to_read {
                self.ledger.wait_for_writer().await;
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
        let record_id = token.record_id();
        let record_len = token.record_len() as u64;
        if self.ready_to_read {
            trace!(
                "read record ID {} with total size {}",
                record_id,
                record_len
            );
        }

        self.update_reader_last_record_id(record_id)
            .await
            .context(IoSnafu)?;
        self.track_read(record_len);
        let reader = self
            .reader
            .as_mut()
            .expect("reader should exist after `ensure_ready_for_read`");
        reader.read_record(token).map(Some)
    }
}
