use std::{
    cmp, fmt,
    io::{self, ErrorKind},
    marker::PhantomData,
    num::NonZeroU64,
    sync::Arc,
};

use crc32fast::Hasher;
use rkyv::{AlignedVec, archived_root};
use snafu::{ResultExt, Snafu};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use vector_common::{finalization::BatchNotifier, finalizer::OrderedFinalizer};

use super::{
    Filesystem,
    common::create_crc32c_hasher,
    ledger::Ledger,
    record::{ArchivedRecord, Record, RecordStatus, validate_record_archive},
};
use crate::{
    Bufferable,
    encoding::{AsMetadata, Encodable},
    internal_events::BufferReadError,
    topology::acks::{EligibleMarker, EligibleMarkerLength, MarkerError, OrderedAcknowledgements},
    variants::disk_v2::{io::AsyncFile, record::try_as_record_archive},
};

pub(super) struct ReadToken {
    record_id: u64,
    record_bytes: usize,
}

impl ReadToken {
    pub fn new(record_id: u64, record_bytes: usize) -> Self {
        Self {
            record_id,
            record_bytes,
        }
    }

    pub fn record_id(&self) -> u64 {
        self.record_id
    }

    pub fn record_bytes(&self) -> usize {
        self.record_bytes
    }

    fn into_record_id(self) -> u64 {
        self.record_id
    }
}

/// Error that occurred during calls to [`BufferReader`].
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
    /// This indicates that a complete length-delimited record was read, but its archive was not
    /// valid. The record's byte span is consumed and removed from buffer occupancy before the
    /// recoverable error is returned, so a later call can continue with the next record.
    #[snafu(display("failed to deserialize encoded record from buffer: {}", reason))]
    Deserialization { reason: String, record_bytes: usize },

    /// The record's checksum did not match.
    ///
    /// The complete length-delimited record is consumed and removed from buffer occupancy before
    /// the recoverable error is returned, so a later call can continue with the next record.
    #[snafu(display(
        "calculated checksum did not match the actual checksum: ({} vs {})",
        calculated,
        actual
    ))]
    Checksum {
        calculated: u32,
        actual: u32,
        record_bytes: usize,
    },

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

    /// The record reported an event count of zero.
    ///
    /// Empty records should not be allowed to be written, so this represents either a bug with the
    /// writing logic of the buffer, or a record that does not use a symmetrical encoding scheme,
    /// which is also not supported.
    EmptyRecord,
}

impl<T> ReaderError<T>
where
    T: Bufferable,
{
    pub(super) fn is_bad_read(&self) -> bool {
        matches!(
            self,
            ReaderError::Checksum { .. }
                | ReaderError::Deserialization { .. }
                | ReaderError::PartialWrite
        )
    }

    pub(super) fn consumed_record_bytes(&self) -> Option<u64> {
        match self {
            ReaderError::Checksum { record_bytes, .. }
            | ReaderError::Deserialization { record_bytes, .. } => Some(
                u64::try_from(*record_bytes)
                    .expect("Vector only supports record sizes that fit in u64"),
            ),
            _ => None,
        }
    }

    fn as_error_code(&self) -> &'static str {
        match self {
            ReaderError::Io { .. } => "io_error",
            ReaderError::Deserialization { .. } => "deser_failed",
            ReaderError::Checksum { .. } => "checksum_mismatch",
            ReaderError::Decode { .. } => "decode_failed",
            ReaderError::Incompatible { .. } => "incompatible_record_version",
            ReaderError::PartialWrite => "partial_write",
            ReaderError::EmptyRecord => "empty_record",
        }
    }

    pub fn as_recoverable_error(&self) -> Option<BufferReadError> {
        let error = self.to_string();
        let error_code = self.as_error_code();

        match self {
            ReaderError::Io { .. } | ReaderError::EmptyRecord => None,
            ReaderError::Deserialization { .. }
            | ReaderError::Checksum { .. }
            | ReaderError::Decode { .. }
            | ReaderError::Incompatible { .. }
            | ReaderError::PartialWrite => Some(BufferReadError { error_code, error }),
        }
    }
}

impl<T: Bufferable> PartialEq for ReaderError<T> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Io { source: l_source }, Self::Io { source: r_source }) => {
                l_source.kind() == r_source.kind()
            }
            (
                Self::Deserialization {
                    reason: l_reason,
                    record_bytes: l_record_bytes,
                },
                Self::Deserialization {
                    reason: r_reason,
                    record_bytes: r_record_bytes,
                },
            ) => l_reason == r_reason && l_record_bytes == r_record_bytes,
            (
                Self::Checksum {
                    calculated: l_calculated,
                    actual: l_actual,
                    record_bytes: l_record_bytes,
                },
                Self::Checksum {
                    calculated: r_calculated,
                    actual: r_actual,
                    record_bytes: r_record_bytes,
                },
            ) => {
                l_calculated == r_calculated
                    && l_actual == r_actual
                    && l_record_bytes == r_record_bytes
            }
            (Self::Decode { .. }, Self::Decode { .. }) => true,
            (Self::Incompatible { reason: l_reason }, Self::Incompatible { reason: r_reason }) => {
                l_reason == r_reason
            }
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

/// Buffered reader that handles deserialization, checksumming, and decoding of records.
pub(super) struct RecordReader<R, T> {
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
    /// Creates a new [`RecordReader`] around the provided reader.
    ///
    /// Internally, the reader is wrapped in a [`BufReader`], so callers should not pass in an
    /// already buffered reader.
    pub fn new(reader: R) -> Self {
        Self {
            reader: BufReader::with_capacity(256 * 1024, reader),
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
    /// Records are preceded by a length delimiter, a fixed-size integer (currently 8 bytes) that
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
        let Some(record_len) = self.read_length_delimiter(is_finalized).await? else {
            return Ok(None);
        };

        if record_len == 0 {
            return Err(ReaderError::Deserialization {
                reason: "record length was zero".to_string(),
                record_bytes: 8,
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
        // TODO: Another spot where our hardcoding of the length delimiter size in bytes is fragile.
        let record_bytes = 8 + buf.len();
        match validate_record_archive(buf, &self.checksummer) {
            RecordStatus::FailedDeserialization(de) => Err(ReaderError::Deserialization {
                reason: de.into_inner(),
                record_bytes,
            }),
            RecordStatus::Corrupted { calculated, actual } => Err(ReaderError::Checksum {
                calculated,
                actual,
                record_bytes,
            }),
            RecordStatus::Valid { id, .. } => {
                self.current_record_id = id;
                Ok(Some(ReadToken::new(id, record_bytes)))
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

        decode_record_payload(record)
    }
}

impl<R, T> fmt::Debug for RecordReader<R, T>
where
    R: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecordReader")
            .field("reader", &self.reader)
            .field("aligned_buf", &self.aligned_buf)
            .field("checksummer", &self.checksummer)
            .field("current_record_id", &self.current_record_id)
            .finish()
    }
}

/// Reads records from the buffer.
#[derive(Debug)]
pub struct BufferReader<T, FS>
where
    FS: Filesystem,
{
    ledger: Arc<Ledger<FS>>,
    reader: Option<RecordReader<FS::File, T>>,
    bytes_read: u64,
    last_reader_record_id: u64,
    data_file_start_record_id: Option<u64>,
    data_file_record_count: u64,
    data_file_marked_record_count: u64,
    ready_to_read: bool,
    record_acks: OrderedAcknowledgements<u64, u64>,
    data_file_acks: OrderedAcknowledgements<u64, ()>,
    finalizer: OrderedFinalizer<u64>,
    _t: PhantomData<T>,
}

impl<T, FS> BufferReader<T, FS>
where
    T: Bufferable,
    FS: Filesystem,
    FS::File: Unpin,
{
    /// Creates a new [`BufferReader`] attached to the given [`Ledger`].
    pub(crate) fn new(ledger: Arc<Ledger<FS>>, finalizer: OrderedFinalizer<u64>) -> Self {
        let ledger_last_reader_record_id = ledger.state().get_last_reader_record_id();
        let next_expected_record_id = ledger_last_reader_record_id + 1;

        Self {
            ledger,
            reader: None,
            bytes_read: 0,
            last_reader_record_id: 0,
            data_file_start_record_id: None,
            data_file_record_count: 0,
            data_file_marked_record_count: 0,
            ready_to_read: false,
            record_acks: OrderedAcknowledgements::from_acked(next_expected_record_id),
            data_file_acks: OrderedAcknowledgements::from_acked(0),
            finalizer,
            _t: PhantomData,
        }
    }

    pub(super) fn ledger(&self) -> &Ledger<FS> {
        self.ledger.as_ref()
    }

    fn reset(&mut self) {
        self.reader = None;
        self.bytes_read = 0;
        self.data_file_start_record_id = None;
    }

    fn track_read(&mut self, record_id: u64, record_bytes: u64, event_count: NonZeroU64) {
        // We explicitly reduce the event count by one here in order to correctly calculate the
        // "last" record ID, which you can visualize as follows...
        //
        // [ record 1 ] [ record 2 ] [ record 3 ] [....]
        // [0]          [1] [2] [3]  [4] [5]      [....]
        //
        // For each of these records, their "last ID" is simply the ID of the first event within the
        // record, plus the event count, minus one.  Another way to look at it is that the "last"
        // reader record ID is always one behind the next expected record ID.  In the above example,
        // the next record ID we would expect would be 6, regardless of how many events the record has.
        self.last_reader_record_id = record_id + event_count.get() - 1;
        if self.data_file_start_record_id.is_none() {
            self.data_file_start_record_id = Some(record_id);
        }

        // Track the amount of data consumed within the current data file. During initialization,
        // this still positions the reader at the persisted checkpoint, but the authoritative unread
        // buffer size is computed separately from the checkpointed file window.
        self.bytes_read += record_bytes;
        if !self.ready_to_read {
            // During initialization we are only replaying already-read records to reposition the
            // reader; we deliberately do NOT adjust `total_buffer_size` here. The authoritative
            // unread total is installed once, at the end of `seek_to_next_record`, from a single
            // consistent snapshot. Decrementing per-record during this replay was the historical
            // approach, and -- because the seeded total and the replayed records came from
            // snapshots taken at different times across a crash -- it is what allowed the
            // buffer-size counter to underflow and wrap.
            return;
        }

        // We've done a "real" record read, so we need to track it for acknowledgement.  Check our
        // acknowledge state first to see if this is the next record ID we expected.
        self.data_file_record_count += 1;
        if let Err(me) =
            self.record_acks
                .add_marker(record_id, Some(event_count.get()), Some(record_bytes))
        {
            match me {
                MarkerError::MonotonicityViolation => {
                    // Reaching here means the reader saw a record id that did not
                    // advance past the last acked id, which crashes the process and
                    // can wedge it in a restart loop. The assertion files the state
                    // before the panic discards it.
                    #[cfg(feature = "antithesis-disk-asserts")]
                    {
                        #![allow(clippy::disallowed_types)] // once_cell::Lazy
                        antithesis_sdk::assert_unreachable!(
                            "reader never sees a record id that breaks monotonicity",
                            &serde_json::json!({
                                "record_id": record_id,
                                "last_reader_record_id": self.last_reader_record_id,
                            })
                        );
                    }
                    panic!("record ID monotonicity violation detected; this is a serious bug")
                }
            }
        }
    }

    fn track_dropped_read(&mut self, record_bytes: u64) {
        // The record boundary was valid, so the reader has consumed these bytes even though the
        // payload could not be delivered. Keep the file cursor accounting complete so a later
        // abandoned-tail adjustment cannot count the same bytes again.
        self.bytes_read += record_bytes;
        if !self.ready_to_read {
            return;
        }

        // Dropped records never enter the acknowledgement path. Remove their bytes immediately
        // from both backpressure and usage accounting, then wake a writer that may be waiting for
        // buffer capacity.
        self.ledger.track_dropped_record_bytes(record_bytes);
        self.ledger.notify_reader_waiters();
    }

    #[cfg_attr(test, instrument(skip_all, level = "debug"))]
    async fn account_abandoned_data_file_tail(&mut self) -> io::Result<()> {
        if !self.ready_to_read {
            return Ok(());
        }

        let data_file_path = self.ledger.get_current_reader_data_file_path();
        let bytes_read = self.bytes_read;
        debug!(
            data_file_path = data_file_path.to_string_lossy().as_ref(),
            bytes_read, "Accounting abandoned data file tail before rolling reader."
        );

        let data_file = self
            .ledger
            .filesystem()
            .open_file_readable(&data_file_path)
            .await?;
        let metadata = data_file.metadata().await?;

        // A file shorter than bytes_read makes the delta below underflow
        // and feed a wrapped value into decrement_total_buffer_size.
        #[cfg(feature = "antithesis-disk-asserts")]
        {
            #![allow(clippy::disallowed_types)] // once_cell::Lazy
            antithesis_sdk::assert_always_greater_than_or_equal_to!(
                metadata.len(),
                bytes_read,
                "reader data-file size delta never underflows"
            );
        }
        let abandoned_tail_bytes = metadata.len().saturating_sub(bytes_read);
        if abandoned_tail_bytes > 0 {
            debug!(
                actual_file_size = metadata.len(),
                bytes_read,
                abandoned_tail_bytes,
                "Data file tail was abandoned after bad read. Adjusting buffer size to compensate.",
            );
            self.ledger
                .decrement_total_buffer_size(abandoned_tail_bytes);
        }

        Ok(())
    }

    #[cfg_attr(test, instrument(skip(self), level = "debug"))]
    fn handle_pending_acknowledgements(
        &mut self,
        force_check_pending_data_files: bool,
    ) -> io::Result<()> {
        // Acknowledgements effectively happen in two layers: record acknowledgement and data file
        // acknowledgement.  Since records can contain multiple events, we need to track when a
        // record itself has been fully acknowledged.  Likewise, data files contain multiple records,
        // so we need to track when all records we've read from a data file have been acknowledged.

        // Drive record acknowledgement first.
        //
        // We only do this if we actually consume any acknowledgements, and immediately update the
        // buffer and ledger to more quickly get those metrics into good shape.  We defer notifying
        // writers until after, though, in case we also complete any data files, so that we can
        // coalesce the notifications together at the very end of the method.
        let mut had_eligible_records = false;
        let mut records_acknowledged: u64 = 0;
        let mut events_acknowledged: u64 = 0;
        let mut events_skipped: u64 = 0;
        let mut bytes_acknowledged: u64 = 0;

        let consumed_acks = self.ledger.consume_pending_acks();
        if consumed_acks > 0 {
            self.record_acks.add_acknowledgements(consumed_acks);

            while let Some(EligibleMarker { len, data, .. }) =
                self.record_acks.get_next_eligible_marker()
            {
                had_eligible_records = true;

                match len {
                    // Any marker with an assumed length implies a gap marker, which gets added
                    // automatically and represents a portion of the record ID range that was
                    // expected but missing. This is a long way of saying: we're missing records.
                    //
                    // We tally this up so that we can emit a single log event/set of metrics, as
                    // there may be many gap markers and emitting for each of them could be very noisy.
                    EligibleMarkerLength::Assumed(count) => {
                        events_skipped = events_skipped
                            .checked_add(count)
                            .expect("skipping more than 2^64 events at a time is obviously a bug");
                    }
                    // We got a valid marker representing a known number of events.
                    EligibleMarkerLength::Known(len) => {
                        // We specifically pass the size of the record, in bytes, as the marker data.
                        let record_bytes = data.expect("record bytes should always be known");

                        records_acknowledged = records_acknowledged.checked_add(1).expect(
                            "acknowledging more than 2^64 records at a time is obviously a bug",
                        );
                        events_acknowledged = events_acknowledged.checked_add(len).expect(
                            "acknowledging more than 2^64 events at a time is obviously a bug",
                        );
                        bytes_acknowledged = bytes_acknowledged.checked_add(record_bytes).expect(
                            "acknowledging more than 2^64 bytes at a time is obviously a bug",
                        );
                    }
                }
            }

            // We successfully processed at least one record, so update our buffer and ledger accounting.
            if had_eligible_records {
                self.ledger
                    .track_reads(events_acknowledged, bytes_acknowledged);

                // We need to account for skipped events, too, so that our "last reader record ID"
                // value stays correct as we process these gap markers.
                let last_increment_amount = events_acknowledged + events_skipped;
                self.ledger
                    .state()
                    .increment_last_reader_record_id(last_increment_amount);

                self.data_file_acks
                    .add_acknowledgements(records_acknowledged);
            }

            // If any events were skipped, do our logging/metrics for that.
            if events_skipped > 0 {
                self.ledger.track_dropped_events(events_skipped);
            }
        }

        // If we processed any eligible records, we may now also have eligible data files.
        //
        // Alternatively, the core `next` logic may have just rolled over to a new data file, and
        // we're seeing if we can fast track any eligible data file completions rather than waiting
        // for more acknowledgements to come in.
        let mut had_eligible_data_files = false;
        let mut data_files_completed: u16 = 0;

        if had_eligible_records || force_check_pending_data_files {
            // Now handle data file completion.  We unconditionally check to see if any data files are
            // eligible for logical completion, and process them immediately. Physical deletion is
            // handled by the background cleanup task.

            let mut completed_data_files = 0u16;
            while let Some(EligibleMarker { .. }) = self.data_file_acks.get_next_eligible_marker() {
                had_eligible_data_files = true;

                completed_data_files = completed_data_files
                    .checked_add(1)
                    .expect("completing more than 2^16 data files at a time is obviously a bug");
            }

            if had_eligible_data_files {
                // Advance every logically completed file before flushing so the durable file
                // checkpoint cannot lag behind the record checkpoint if we crash before deletion.
                for _ in 0..completed_data_files {
                    self.ledger.increment_acked_reader_file_id();
                }
                self.ledger.flush_reader_file_checkpoint()?;

                data_files_completed = completed_data_files;
            }
        }

        // If we managed to process any records _or_ any data file completions, we've made
        // meaningful progress that writers may care about, so notify them.
        if had_eligible_data_files || had_eligible_records {
            self.ledger.notify_reader_waiters();

            if self.ready_to_read {
                trace!(
                    current_buffer_size = self.ledger.get_total_buffer_size(),
                    records_acknowledged,
                    events_acknowledged,
                    events_skipped,
                    bytes_acknowledged,
                    data_files_completed,
                    "Finished handling acknowledgements."
                );
            }
        }

        Ok(())
    }

    /// Switches the reader over to the next data file to read.
    #[cfg_attr(test, instrument(skip(self), level = "debug"))]
    fn roll_to_next_data_file(&mut self) {
        // Add a marker for this data file so we know when every record read from it has been
        // acknowledged, at which point we can durably advance the reader file checkpoint.
        //
        // In the rare case where the very first read in a new data file is corrupted/invalid and we
        // roll to the next data file, we simply use the last reader record ID we have, which yields
        // a marker with a length of 0.
        let data_file_start_record_id = self
            .data_file_start_record_id
            .take()
            .unwrap_or(self.last_reader_record_id);
        // Record IDs are inclusive, so if last is 1 and start is 0, that means we had two events,
        // potentially from one or two records.
        let data_file_event_count = self.last_reader_record_id - data_file_start_record_id + 1;
        let data_file_record_count = self.data_file_record_count;
        let data_file_path = self.ledger.get_current_reader_data_file_path();
        let bytes_read = self.bytes_read;

        debug!(
            data_file_path = data_file_path.to_string_lossy().as_ref(),
            first_record_id = data_file_start_record_id,
            last_record_id = self.last_reader_record_id,
            record_count = data_file_record_count,
            event_count = data_file_event_count,
            bytes_read,
            "Marking data file for completion."
        );

        let data_file_marker_id = self.data_file_marked_record_count;
        self.data_file_marked_record_count += data_file_record_count;
        self.data_file_record_count = 0;

        self.data_file_acks
            .add_marker(data_file_marker_id, Some(data_file_record_count), None)
            .expect("should not fail to add marker for data file completion");

        // Now reset our internal state so we can go for the next data file.
        self.reset();
        self.ledger.increment_unacked_reader_file_id();

        debug!("Rolling to next data file.");
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
            let data_file = match self
                .ledger
                .filesystem()
                .open_file_readable(&data_file_path)
                .await
            {
                Ok(data_file) => data_file,
                Err(e) => match e.kind() {
                    ErrorKind::NotFound => {
                        // reader is either waiting for writer to create the file which can be current writer_file_id or next writer_file_id (if writer has marked for skip)
                        if reader_file_id == writer_file_id
                            || reader_file_id == self.ledger.get_next_writer_file_id()
                        {
                            debug!(
                                data_file_path = data_file_path.to_string_lossy().as_ref(),
                                "Data file does not yet exist. Waiting for writer to create."
                            );
                            self.ledger.wait_for_writer().await;
                        } else {
                            self.ledger.increment_acked_reader_file_id();
                        }
                        continue;
                    }
                    // This is a valid I/O error, so bubble that back up.
                    _ => return Err(e),
                },
            };

            debug!(
                data_file_path = data_file_path.to_string_lossy().as_ref(),
                "Opened data file for reading."
            );

            self.reader = Some(RecordReader::new(data_file));
            return Ok(());
        }
    }

    /// Seeks to where this reader previously left off.
    ///
    /// In cases where Vector has restarted, but the reader hasn't yet finished a file, we would
    /// open the correct data file for reading, but our file cursor would be at the very
    /// beginning, essentially pointed at the wrong record.  We read out records here until we
    /// reach a point where we've read up to the record referenced by `get_last_reader_record_id`.
    ///
    /// This ensures that a subsequent call to `next` is ready to read the correct record.
    ///
    /// # Errors
    ///
    /// If an error occurs during seeking to the next record, an error variant will be returned
    /// describing the error.
    #[cfg_attr(test, instrument(skip(self), level = "debug"))]
    pub(super) async fn seek_to_next_record(&mut self) -> Result<(), ReaderError<T>> {
        // We don't try seeking again once we're all caught up.
        if self.ready_to_read {
            warn!("Reader already initialized.");
            return Ok(());
        }

        // We rely on `next` to close out the data file if we've actually reached the end, and we
        // also rely on it to reset the data file before trying to read, and we _also_ rely on it to
        // update `self.last_reader_record_id`, so basically... just keep reading records until we
        // get to the one we left off with last time.
        let ledger_last = self.ledger.state().get_last_reader_record_id();
        debug!(
            last_acknowledged_record_id = ledger_last,
            "Seeking to last acknowledged record for reader."
        );

        // We may end up in a situation where a data file hasn't yet been physically deleted but
        // we've moved on to the next data file, including acknowledging records within it. If
        // Vector is stopped at a point like this, and we restart it and load the buffer, we'll start
        // on the old data file. That's wasteful to read all over again.
        //
        // In our seek loop, we have a fast path where we check the last record of a data file while
        // the reader and writer file IDs don't match. If we see that the record is still below the
        // last reader record ID, we durably advance the reader file checkpoint and move to the next
        // file. This is safe because we know that if we managed to acknowledge records with an ID
        // higher than the highest record ID in the data file, it was fully consumed.
        //
        // Once the reader/writer file IDs are identical, we fall back to the slow path.
        while self.ledger.get_current_reader_file_id() != self.ledger.get_current_writer_file_id() {
            let data_file_path = self.ledger.get_current_reader_data_file_path();
            self.ensure_ready_for_read().await.context(IoSnafu)?;
            let data_file_mmap = self
                .ledger
                .filesystem()
                .open_mmap_readable(&data_file_path)
                .await
                .context(IoSnafu)?;

            match validate_record_archive(data_file_mmap.as_ref(), &Hasher::new()) {
                RecordStatus::Valid {
                    id: last_record_id, ..
                } => {
                    let record = try_as_record_archive(data_file_mmap.as_ref())
                        .expect("record was already validated");

                    let Ok(item) = decode_record_payload::<T>(record) else {
                        // If there's an error decoding the item, just fall back to the slow path,
                        // because this file might actually be where we left off, so we don't want
                        // to incorrectly skip ahead or anything.
                        break;
                    };

                    // We have to remove 1 from the event count here because otherwise the ID would
                    // be the _next_ record's ID we'd expect, not the last ID of the record we are
                    // acknowledged up to. (Record IDs start at N and consume up to N+M-1 where M is
                    // the number of events in the record, which is how we can determine the event
                    // count from the record IDs alone, without having to read every record in the
                    // buffer during startup.)
                    let record_events = u64::try_from(item.event_count())
                        .expect("event count should never exceed u64");
                    let last_record_id_in_data_file =
                        last_record_id + record_events.saturating_sub(1);

                    // If we're past this data file, mark it complete and move on. We do this
                    // manually versus faking it via `roll_to_next_data_file` because that emits an
                    // acknowledgement marker, but the internal state tracking first/last record ID,
                    // bytes read, etc, won't actually be usable.
                    if ledger_last > last_record_id_in_data_file {
                        // This fully-read file is behind `ledger_last`, so startup can advance its
                        // logical reader file checkpoint. Buffer size is recomputed once
                        // `seek_to_next_record` finishes positioning the reader.
                        self.ledger.increment_acked_reader_file_id();
                        self.ledger
                            .flush_reader_file_checkpoint()
                            .context(IoSnafu)?;
                        self.reset();
                    } else {
                        // We've hit a point where the current data file we're on has records newer
                        // than where we left off, so we can catch up from here.
                        break;
                    }
                }
                // Similar to the comment above about when decoding fails, we fallback to the slow
                // path in case any error is encountered, lest we risk incorrectly skipping ahead to
                // the wrong data file.
                _ => break,
            }
        }

        // We rely on `next` to close out the data file if we've actually reached the end, and we
        // also rely on it to reset the data file before trying to read, and we _also_ rely on it to
        // update `self.last_reader_record_id`, so basically... just keep reading records until
        // we're past the last record we had acknowledged.
        while self.last_reader_record_id < ledger_last {
            match self.next().await {
                Ok(maybe_record) => {
                    if maybe_record.is_none() {
                        // We've hit the end of the current data file so we've gone as far as we can.
                        break;
                    }
                }
                Err(e) if e.is_bad_read() => {
                    // If we hit a bad read during initialization, we should only continue calling
                    // `next` if we have not advanced _past_ the writer in terms of file ID.
                    //
                    // If the writer saw the same error we just saw, it will have rolled itself to
                    // the next file, lazily: for example, it discovers a bad record at the end of
                    // file ID 3, so it marks itself to open file ID 4 next, but hasn't yet
                    // created it, and is still technically indicated as being on file ID 3.
                    //
                    // Meanwhile, if _we_ try to also roll to file ID 4 and read from it, we'll deadlock
                    // ourselves because it doesn't yet exist. However, `next` immediately updates our
                    // reader file ID as soon as it hits a bad read error, so in this scenario,
                    // we're now marked as being on file ID 4 while the writer is still on file ID
                    // 3.
                    //
                    // From that, we can determine that when we've hit a bad read error, that if our
                    // file ID is greater than the writer's file ID, we're now essentially
                    // synchronized.
                    let (reader_file_id, writer_file_id) =
                        self.ledger.get_current_reader_writer_file_id();
                    if reader_file_id > writer_file_id {
                        break;
                    }
                }
                Err(e) => return Err(e),
            }
        }

        debug!(
            last_record_id_read = self.last_reader_record_id,
            "Synchronized with ledger. Reader ready."
        );

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
    // The inline antithesis assertion blocks push this over the line limit. Their
    // source lines count even when the feature is off, so the allow is unconditional.
    #[allow(clippy::too_many_lines)]
    pub async fn next(&mut self) -> Result<Option<T>, ReaderError<T>> {
        let mut force_check_pending_data_files = false;

        let token = loop {
            // Handle any pending acknowledgements first.
            self.handle_pending_acknowledgements(force_check_pending_data_files)
                .context(IoSnafu)?;
            force_check_pending_data_files = false;

            // If the writer has marked themselves as done, and the buffer has been emptied, then
            // we're done and can return.  We have to look at something besides simply the writer
            // being marked as done to know if we're actually done or not, and "buffer size" is better
            // than "total records" because buffer size is decremented when records are acknowledged
            // and when the reader abandons a bad tail.
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

            // Essentially: is the writer still writing to this data file or not, and are we
            // actually ready to read (aka initialized)?
            //
            // This is a necessary invariant to understand if the record reader should actually keep
            // waiting for data, or if a data file had a partial write/missing data and should be
            // skipped. In particular, not only does this matter for deadlocking during shutdown due
            // to improper writer behavior/flushing, but it also matters during initialization in
            // case where the current data file had a partial write.
            let is_finalized = (reader_file_id != writer_file_id) || !self.ready_to_read;

            // Try reading a record, which if successful, gives us a token to actually read/get a
            // reference to the record.  This is a slightly-tricky song-and-dance due to rustc not
            // yet fully understanding mutable borrows when conditional control flow is involved.
            match reader.try_next_record(is_finalized).await {
                // Not even enough data to read a length delimiter, so we need to wait for the
                // writer to signal us that there's some actual data to read.
                Ok(None) => {}
                // We got a valid record, so keep the token.
                Ok(Some(token)) => break token,
                // A complete length-delimited payload that fails archive or checksum validation
                // still has a trustworthy consumed byte span. Drop and account for that record,
                // then leave the reader positioned at the next frame. Only an incomplete frame
                // requires abandoning the remaining file tail.
                Err(e) => {
                    if let Some(record_bytes) = e.consumed_record_bytes() {
                        self.track_dropped_read(record_bytes);
                    } else if e.is_bad_read() {
                        // The reader hit a corrupted, torn, or partially-written
                        // record and is abandoning the rest of this file, the
                        // recovery path that drives the skip-accounting hazards.
                        #[cfg(feature = "antithesis-disk-asserts")]
                        {
                            #![allow(clippy::disallowed_types)] // once_cell::Lazy
                            antithesis_sdk::assert_sometimes!(
                                true,
                                "the reader skips a torn or corrupted record and rolls the file",
                                &serde_json::json!({
                                    "last_reader_record_id": self.last_reader_record_id,
                                })
                            );
                        }
                        self.account_abandoned_data_file_tail()
                            .await
                            .context(IoSnafu)?;
                        self.roll_to_next_data_file();
                    }

                    return Err(e);
                }
            }

            // Fundamentally, when `try_read_record` returns `None`, there's three possible
            // scenarios:
            //
            // 1. we are entirely caught up to the writer
            // 2. we've hit the end of the data file and need to go to the next one
            // 3. the writer has closed/dropped/finished/etc
            //
            // When we're at this point, we check the reader/writer file IDs.  If the file IDs are
            // not identical, we now know the writer has moved on.  Crucially, since we always flush
            // our writes before waking up, including before moving to a new file, then we know that
            // if the reader/writer were not identical at the start the loop, and `try_read_record`
            // returned `None`, that we have hit the actual end of the reader's current data file,
            // and need to move on.
            //
            // If the file IDs were identical, it would imply that reader is still on the writer's
            // current data file. We then "wait" for the writer to wake us up. It may lead to the
            // same thing -- `try_read_record` returning `None` with an identical reader/writer file
            // ID -- but that's OK, because it would mean we were actually waiting for the writer to
            // make progress now.  If the wake-up was valid, due to writer progress, then, well...
            // we'd actually be able to read data.
            //
            // The case of "the writer has closed/dropped/finished/etc" is handled at the top of the
            // loop, because otherwise we could get stuck waiting for the writer after an empty
            // `try_read_record` attempt when the writer is done and we're at the end of the file,
            // etc.
            if self.ready_to_read {
                if reader_file_id != writer_file_id {
                    debug!(
                        reader_file_id,
                        writer_file_id, "Reached the end of current data file."
                    );

                    self.roll_to_next_data_file();
                    force_check_pending_data_files = true;
                    continue;
                }

                self.ledger.wait_for_writer().await;
            } else {
                debug!(
                    bytes_read = self.bytes_read,
                    "Current data file has no more data."
                );

                if reader_file_id == writer_file_id {
                    // We're currently just seeking to where we left off the last time this buffer was
                    // running, which might mean there's no records for us to read at all because we
                    // were already caught up.  All we can do is signal to `seek_to_next_record` that
                    // we're caught up.
                    return Ok(None);
                }

                // The durable record checkpoint can be ahead of its file checkpoint. If startup
                // had to scan this finalized file -- for example, because its last complete frame
                // was corrupt -- reaching EOF proves that the reader can advance to the next file
                // and continue seeking toward that record checkpoint.
                self.roll_to_next_data_file();
                force_check_pending_data_files = true;
            }
        };

        // We got a read token, so our record is present in the reader, and now we can actually read
        // it out and return it.
        let record_id = token.record_id();
        let record_bytes = token.record_bytes() as u64;

        let read_result = self
            .reader
            .as_mut()
            .expect("reader should exist after `ensure_ready_for_read`")
            .read_record(token);
        let mut record = match read_result {
            Ok(record) => record,
            Err(error) => {
                self.track_dropped_read(record_bytes);
                return Err(error);
            }
        };

        // A record only reaches delivery after `try_next_record` accepted it on the
        // `Valid` arm, which rejects a zero-length record and issues a token whose
        // byte count is the eight-byte length delimiter plus the validated payload.
        // A delivered record whose consumed span is at most the bare delimiter means
        // a corrupted or empty record slipped past validation.
        #[cfg(feature = "antithesis-disk-asserts")]
        {
            #![allow(clippy::disallowed_types)] // once_cell::Lazy
            antithesis_sdk::assert_always_or_unreachable!(
                record_bytes > 8,
                "no corrupted or empty record is delivered to the reader",
                &serde_json::json!({
                    "record_id": record_id,
                    "record_bytes": record_bytes,
                })
            );
        }

        let record_events: u64 = record
            .event_count()
            .try_into()
            .expect("Event count for a record cannot exceed 2^64 events.");
        let Ok(record_events) = record_events.try_into() else {
            self.track_dropped_read(record_bytes);
            return Err(ReaderError::EmptyRecord);
        };
        self.track_read(record_id, record_bytes, record_events);

        let (batch, receiver) = BatchNotifier::new_with_receiver();
        record.add_batch_notifier(batch);
        self.finalizer.add(record_events.get(), receiver);

        if self.ready_to_read {
            trace!(
                record_id,
                record_events,
                record_bytes,
                data_file_id = self.ledger.get_current_reader_file_id(),
                "Read record."
            );
        }

        Ok(Some(record))
    }
}

pub(crate) fn decode_record_payload<T: Bufferable>(
    record: &ArchivedRecord<'_>,
) -> Result<T, ReaderError<T>> {
    // Try and convert the raw record metadata into the true metadata type used by `T`, and then
    // also verify that `T` is able to decode records with the metadata used for this record in particular.
    let metadata = T::Metadata::from_u32(record.metadata()).ok_or(ReaderError::Incompatible {
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
