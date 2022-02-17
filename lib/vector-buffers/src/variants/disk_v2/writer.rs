use std::{
    cmp::Ordering,
    io::{self, ErrorKind},
    marker::PhantomData,
    num::NonZeroUsize,
    sync::Arc,
};

use bytes::BufMut;
use crc32fast::Hasher;
use memmap2::Mmap;
use rkyv::{
    ser::{
        serializers::{
            AlignedSerializer, AllocScratch, BufferScratch, CompositeSerializer,
            CompositeSerializerError, FallbackScratch,
        },
        Serializer,
    },
    AlignedVec, Infallible,
};
use snafu::{ResultExt, Snafu};
use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncWrite, AsyncWriteExt, BufWriter},
};

use super::{
    common::{create_crc32c_hasher, DiskBufferConfig},
    ledger::Ledger,
    record::{validate_record_archive, Record, RecordStatus},
};
use crate::{
    encoding::{AsMetadata, Encodable},
    variants::disk_v2::{reader::decode_record_payload, record::try_as_record_archive},
    Bufferable,
};

/// Error that occurred during calls to [`Writer`].
#[derive(Debug, Snafu)]
pub enum WriterError<T>
where
    T: Bufferable,
{
    /// A general I/O error occurred.
    ///
    /// Different methods will capture specific I/O errors depending on the situation, as some
    /// errors may be expected and considered normal by design.  For all I/O errors that are
    /// considered atypical, they will be returned as this variant.
    #[snafu(display("write I/O error: {}", source))]
    Io { source: io::Error },

    /// The record attempting to be written was too large.
    ///
    /// In practice, most encoders will throw their own error if they cannot write all of the
    /// necessary bytes during encoding, and so this error will typically only be emitted when the
    /// encoder throws no error during the encoding step itself, but manages to fill up the encoding
    /// buffer to the limit.
    #[snafu(display("record too large: limit is {}", limit))]
    RecordTooLarge { limit: usize },

    /// The data file did not have enough remaining space to write the record.
    ///
    /// This could be because the data file is legitimately full, but is more commonly related to a
    /// record being big enough that it would exceed the max data file size.
    ///
    /// The record that was given to write is returned.
    ///
    /// See lemma 5.
    #[snafu(display("data file full or record would exceed max data file size"))]
    DataFileFull { record: T, serialized_size: usize },

    /// A record reported that it contained more events than the number of bytes when encoded.
    ///
    /// This is nonsensicial because we don't intend to ever support encoding zero-sized types
    /// through the buffer, and the logic we use to count the number of actual events in the buffer
    /// transitively depends on not being able to represent more than one event per encoded byte.
    ///
    /// See lemma 4 for more information.
    #[snafu(display(
        "record reported event count ({}) higher than encoded length ({})",
        encoded_len,
        event_count
    ))]
    NonsensicalEventCount {
        encoded_len: usize,
        event_count: usize,
    },

    /// The encoder encountered an issue during encoding.
    ///
    /// For common encoders, failure to write all of the bytes of the input will be the most common
    /// error, and in fact, some some encoders, it's the only possible error that can occur.
    #[snafu(display("failed to encode record: {:?}", source))]
    FailedToEncode {
        source: <T as Encodable>::EncodeError,
    },

    /// The writer failed to serialize the record.
    ///
    /// As records are encoded and then wrapped in a container which carries metadata about the size
    /// of the encoded record, and so on, there is a chance that we could fail to serialize that
    /// container during the write step.
    ///
    /// In practice, this should generally only occur if the system is unable to allocate enough
    /// memory during the serialization step aka the system itself is literally out of memory to
    /// give to processes.  Rare, indeed.
    #[snafu(display("failed to serialize encoded record to buffer: {}", reason))]
    FailedToSerialize { reason: String },

    /// The writer failed to validate the last written record.
    ///
    /// Specifically, for `Writer`, this can only ever be returned when creating the buffer, during
    /// validation of the last written record.  While it's technically possible that it may be
    /// something else, this error is most likely to occur when the records in a buffer were written
    /// in a different version of Vector that cannot be decoded in this version of Vector.
    #[snafu(display("failed to validate the last written record: {}", reason))]
    FailedToValidate { reason: String },

    /// The writer entered an inconsistent state that represents an unrecoverable error.
    ///
    /// In some cases, like expecting to be able to decode an event we just encoded, we might hit an
    /// error.  This would be an entirely unexpected error -- how is it possible to not be able to
    /// decode an event we literally just encoded on the line above? -- and as such, the only
    /// reasonable thing to do would be to give up.
    ///
    /// This error is the writer, and thus the buffer, giving up.
    #[snafu(display("writer entered inconsistent state: {}", reason))]
    InconsistentState { reason: String },

    /// The record reported an event count of zero.
    ///
    /// Empty records are not supported.
    EmptyRecord,
}

impl<T> From<io::Error> for WriterError<T>
where
    T: Bufferable,
{
    fn from(source: io::Error) -> Self {
        WriterError::Io { source }
    }
}

/// Buffered writer that handles encoding, checksumming, and serialization of records.
#[derive(Debug)]
pub(super) struct RecordWriter<W, T> {
    writer: BufWriter<W>,
    encode_buf: Vec<u8>,
    ser_buf: AlignedVec,
    ser_scratch: AlignedVec,
    checksummer: Hasher,
    max_record_size: usize,
    current_data_file_size: u64,
    max_data_file_size: u64,
    _t: PhantomData<T>,
}

impl<W, T> RecordWriter<W, T>
where
    W: AsyncWrite + Unpin,
    T: Bufferable,
{
    /// Creates a new [`RecordWriter`] around the provided writer.
    ///
    /// Internally, the writer is wrapped in a [`BufWriter`], so callers should not pass in an
    /// already buffered writer.
    pub fn new(
        writer: W,
        current_data_file_size: u64,
        max_data_file_size: u64,
        max_record_size: usize,
    ) -> Self {
        Self {
            writer: BufWriter::with_capacity(256 * 1024, writer),
            encode_buf: Vec::with_capacity(16_384),
            ser_buf: AlignedVec::with_capacity(16_384),
            ser_scratch: AlignedVec::with_capacity(16_384),
            checksummer: create_crc32c_hasher(),
            max_record_size,
            current_data_file_size,
            max_data_file_size,
            _t: PhantomData,
        }
    }

    /// Gets a reference to the underlying writer.
    pub fn get_ref(&self) -> &W {
        self.writer.get_ref()
    }

    /// Whether or not `amount` bytes could be written while obeying the data file size limit.
    ///
    /// If no bytes have written at all to a data file, then `amount` is allowed to exceed the
    /// limit, otherwise a record would never be able to be written.
    fn can_write(&self, amount: usize) -> bool {
        let amount = u64::try_from(amount)
            .expect("Vector does not yet support running on 128-bit architectures.");
        self.current_data_file_size == 0
            || self.current_data_file_size + amount as u64 <= self.max_data_file_size
    }

    /// Writes a record.
    ///
    /// Returns the number of bytes written to serialize the record, including the framing. Writes
    /// are not automatically flushed, so `flush` must be called after any record write if there is
    /// a requirement for the record to immediately be written all the way to the underlying writer.
    ///
    /// # Errors
    ///
    /// Errors can occur during the encoding, serialization, or I/O stage.  If an error occurs
    /// during any of these stages, an appropriate error variant will be returned describing the error.
    #[instrument(skip(self, record), level = "debug")]
    pub async fn write_record(&mut self, id: u64, record: T) -> Result<usize, WriterError<T>> {
        self.encode_buf.clear();
        self.ser_buf.clear();
        self.ser_scratch.clear();

        // We first encode the record, which puts it into the desired encoded form.  This is where
        // we assert the record is within size limits, etc.
        //
        // NOTE: Some encoders may not write to the buffer in a way that fills it up before
        // themselves returning an error because they know the buffer is too small.  This means we
        // may often return the "failed to encode" error variant when the true error is that the
        // payload size, when encoded, exceeds our limit.
        //
        // Unfortunately, there's not a whole lot for us to do here beyond allowing our buffer to
        // grow beyond the limit so that we can try to allow encoding to succeed so that we can grab
        // the actual encoded size and then check it against the limit.
        //
        // C'est la vie.
        let encode_result = {
            let mut encode_buf = (&mut self.encode_buf).limit(self.max_record_size);
            record.encode(&mut encode_buf)
        };
        let encoded_len = encode_result
            .map(|_| self.encode_buf.len())
            .context(FailedToEncodeSnafu)?;
        if encoded_len >= self.max_record_size {
            return Err(WriterError::RecordTooLarge {
                limit: self.max_record_size,
            });
        }

        let metadata = T::get_metadata().into_u32();
        let wrapped_record =
            Record::with_checksum(id, metadata, &self.encode_buf, &self.checksummer);

        // TODO: This could be a good spot to potentially calculate the on-disk size of the archived
        // record, but to do that correctly, it involves needing a few things:
        // - correctly size the archived record (can already do)
        // - getting the length of DST fields like slices (can do by hand, not resilient to `Record`
        //   changing, though)
        // - getting the serializer alignment  (doable by hand, but also subject to impl details
        //   hidden under-the-hood)
        //
        // Since that stuff is tricky, it's easiest to just rely on the fully serialized archive,
        // although it means we go through the serialization step needlessly when the record
        // inevitably won't fit.  All things considered, though, this will occur very infrequently,
        // though: something like once every ~32k writes if every write is ~4KB.

        // Now serialize the record, which puts it into its archived form.  This is what powers our
        // ability to do zero-copy deserialization from disk.
        //
        // NOTE: This operation is put into its own block scope because otherwise `serializer` lives
        // until the end of the function, and it contains a mutable buffer pointer, which is
        // `!Send` and thus can't move across await points.  Do not rearrange.
        let archive_len = {
            let mut serializer = CompositeSerializer::new(
                AlignedSerializer::new(&mut self.ser_buf),
                FallbackScratch::new(
                    BufferScratch::new(&mut self.ser_scratch),
                    AllocScratch::new(),
                ),
                Infallible,
            );

            match serializer.serialize_value(&wrapped_record) {
                Ok(_) => Ok::<_, WriterError<T>>(serializer.pos()),
                Err(e) => match e {
                    CompositeSerializerError::ScratchSpaceError(sse) => {
                        return Err(WriterError::FailedToSerialize {
                            reason: format!(
                                "insufficient space to serialize encoded record: {}",
                                sse
                            ),
                        })
                    }
                    // Only our scratch space strategy is fallible, so we should never get here.
                    CompositeSerializerError::SerializerError(_)
                    | CompositeSerializerError::SharedError(_) => unreachable!(),
                },
            }
        }?;

        let archive_buf = self.ser_buf.as_slice();
        debug_assert_eq!(archive_buf.len(), archive_len);

        // With the record archived and serialized, do our final check to ensure we can fit this
        // write.  We always allow at least one write into an empty data file.
        //
        // TODO: This is likely to never change, but ugh, this is fragile and I wish we had a
        // better/super low overhead way to capture "the bytes we wrote" rather than piecing
        // together what we _believe_ we should have written.
        let archive_on_disk_len = archive_len + 8;
        if !self.can_write(archive_on_disk_len) {
            debug!(
                current_data_file_size = self.current_data_file_size,
                max_data_file_size = self.max_data_file_size,
                archive_on_disk_len,
                "Archived record is too large to fit in remaining free space of current data file."
            );

            // We have to decode the record back out to actually be able to give it back.  If we
            // can't decode it for some reason, this is entirely an unrecoverable error, since an
            // encoded record should always be decodable within the same process that encoded it.
            let record = T::decode(T::get_metadata(), &self.encode_buf[..]).map_err(|_| {
                WriterError::InconsistentState {
                    reason: "failed to decode record immediately after encoding it".to_string(),
                }
            })?;

            return Err(WriterError::DataFileFull {
                record,
                serialized_size: archive_on_disk_len,
            });
        }

        let wire_archive_len: u64 = archive_len
            .try_into()
            .expect("archive len should always fit into a u64");
        let archive_len_buf = wire_archive_len.to_be_bytes();
        assert_eq!(archive_len_buf[..].len(), 8);

        self.writer
            .write_all(&archive_len_buf)
            .await
            .context(IoSnafu)?;
        self.writer.write_all(archive_buf).await.context(IoSnafu)?;

        // Update our current data file size.
        self.current_data_file_size += wire_archive_len + 8;

        Ok(archive_on_disk_len)
    }

    /// Flushes the writer.
    ///
    /// This flushes both the internal buffered writer and the underlying writer object.
    ///
    /// # Errors
    ///
    /// If there is an I/O error while flushing either the buffered writer or the underlying writer,
    /// an error variant will be returned describing the error.
    #[instrument(skip(self), level = "debug")]
    pub async fn flush(&mut self) -> io::Result<()> {
        self.writer.flush().await
    }
}

impl<T> RecordWriter<File, T> {
    /// Synchronizes the underlying file to disk.
    ///
    /// This tries to synchronize both data and metadata.
    ///
    /// # Errors
    ///
    /// If there is an I/O error while syncing the file, an error variant will be returned
    /// describing the error.
    #[instrument(skip(self), level = "debug")]
    pub async fn sync_all(&mut self) -> io::Result<()> {
        self.writer.get_mut().sync_all().await
    }
}

/// Writes records to the buffer.
#[derive(Debug)]
pub struct Writer<T> {
    ledger: Arc<Ledger>,
    config: DiskBufferConfig,
    writer: Option<RecordWriter<File, T>>,
    data_file_size: u64,
    data_file_full: bool,
    skip_to_next: bool,
    _t: PhantomData<T>,
}

impl<T> Writer<T>
where
    T: Bufferable,
{
    /// Creates a new [`Writer`] attached to the given [`Ledger`].
    pub(crate) fn new(ledger: Arc<Ledger>) -> Self {
        let config = ledger.config().clone();
        Writer {
            ledger,
            config,
            writer: None,
            data_file_size: 0,
            data_file_full: false,
            skip_to_next: false,
            _t: PhantomData,
        }
    }

    #[instrument(skip(self), level = "debug")]
    fn track_write(&mut self, record_len: u64, record_size: u64) {
        self.data_file_size += record_size;
        self.ledger.track_write(record_len, record_size);
    }

    fn can_write(&mut self) -> bool {
        !self.data_file_full && self.data_file_size < self.config.max_data_file_size
    }

    #[instrument(skip(self), level = "debug")]
    fn mark_data_file_full(&mut self) {
        self.data_file_full = true;
    }

    #[instrument(skip(self), level = "debug")]
    fn reset(&mut self) {
        self.writer = None;
        self.data_file_size = 0;
        self.data_file_full = false;
    }

    #[instrument(skip(self), level = "debug")]
    fn mark_for_skip(&mut self) {
        self.skip_to_next = true;
    }

    fn should_skip(&mut self) -> bool {
        let should_skip = self.skip_to_next;
        if should_skip {
            self.skip_to_next = false;
        }

        should_skip
    }

    /// Validates that the last write in the current writer data file matches the ledger.
    ///
    /// # Errors
    ///
    /// If the current data file is not an empty, and there is an error reading it to perform
    /// validation, an error variant will be returned that describes the error.
    ///
    /// Practically speaking, however, this method will only return I/O-related errors as all
    /// logical errors, such as the record being invalid, are captured in order to logically adjust
    /// the writer/ledger state to start a new file, etc.
    #[instrument(skip(self), level = "debug")]
    pub(super) async fn validate_last_write(&mut self) -> Result<(), WriterError<T>> {
        debug!(
            current_writer_data_file = ?self.ledger.get_current_writer_data_file_path(),
            "Validating last written record in current data file."
        );
        self.ensure_ready_for_write().await.context(IoSnafu)?;

        // If our current file is empty, there's no sense doing this check.
        if self.data_file_size == 0 {
            return Ok(());
        }

        // We do a neat little trick here where we open an immutable memory-mapped region against our
        // current writer data file, which lets us treat it as one big buffer... which is useful for
        // asking `rkyv` to deserialize just the last record from the file, without having to seek
        // directly to the start of the record where the length delimiter is.
        let data_file_handle = self
            .writer
            .as_ref()
            .expect("writer should exist after `ensure_ready_for_write`")
            .get_ref()
            .try_clone()
            .await
            .context(IoSnafu)?
            .into_std()
            .await;

        let data_file_mmap = unsafe { Mmap::map(&data_file_handle).context(IoSnafu)? };

        // We have bytes, so we should have an archived record... hopefully!  Go through the motions
        // of verifying it.  If we hit any invalid states, then we should bump to the next data file
        // since the reader will have to stop once it hits the first error in a given file.
        let should_skip_to_next_file = match validate_record_archive(
            data_file_mmap.as_ref(),
            &Hasher::new(),
        ) {
            RecordStatus::Valid {
                id: last_record_id, ..
            } => {
                // We now know the record is valid from the perspective of being framed correctly,
                // and the checksum matching, etc.  We'll attempt to actually decode it now so we
                // can get the actual item that was written, which we need to understand where the
                // next writer record ID should be.
                let record = try_as_record_archive(data_file_mmap.as_ref())
                    .expect("record was already validated");
                let item = decode_record_payload::<T>(record).map_err(|e| {
                    WriterError::FailedToValidate {
                        reason: e.to_string(),
                    }
                })?;

                // Since we have a valid record, checksum and all, see if the writer record ID
                // in the ledger lines up with the record ID we have here.  Specifically, the record
                // ID plus the number of events in the record should be the next record ID that gets used.
                let ledger_next = self.ledger.state().get_next_writer_record_id();
                let record_events =
                    u64::try_from(item.event_count()).expect("event count should never exceed u64");
                let record_next = last_record_id.wrapping_add(record_events);

                match ledger_next.cmp(&record_next) {
                    Ordering::Equal => {
                        // We're exactly where the ledger thinks we should be, so nothing to do.
                        debug!(
                            ledger_next,
                            last_record_id,
                            record_events,
                            "Synchronized with ledger. Writer ready."
                        );
                        false
                    }
                    Ordering::Greater => {
                        // Our last write is behind where the ledger thinks we should be, so we
                        // likely missed flushing some records, or partially flushed the data file.
                        // Better roll over to be safe.
                        error!(
                            ledger_next, last_record_id, record_events,
                            "Last record written to data file is behind expected position. Events have likely been lost.");
                        true
                    }
                    Ordering::Less => {
                        // We're actually _ahead_ of the ledger, which is to say we wrote a valid
                        // record to the data file, but never incremented our "writer next record
                        // ID" field.  Given that record IDs are monotonic, it's safe to forward
                        // ourselves to make the "writer next record ID" in the ledger match the
                        // reality of the data file.  If there were somehow gaps in the data file,
                        // the reader will detect it, and this way, we avoid duplicate record IDs.
                        debug!(
                            ledger_next,
                            last_record_id,
                            record_events,
                            new_ledger_next = record_next,
                            "Ledger desynchronized from data files. Fast forwarding ledger state."
                        );
                        let ledger_record_delta = record_next - ledger_next;
                        self.ledger
                            .state()
                            .increment_next_writer_record_id(ledger_record_delta);
                        false
                    }
                }
            }
            // The record payload was corrupted, somehow: we know the checksum failed to match on
            // both sides, but it could be cosmic radiation that flipped a bit or some process
            // trampled over the data file... who knows.
            //
            // We skip to the next data file to try and start from a clean slate.
            RecordStatus::Corrupted { .. } => {
                error!(
                    "Last written record did not match the expected checksum. Corruption likely."
                );
                true
            }
            // The record itself was corrupted, somehow: it was sufficiently different that `rkyv`
            // couldn't even validate it, which likely means missing bytes but could also be certain
            // bytes being invalid for the struct fields they represent.  Like invalid checksums, we
            // really don't know why it happened, only that it happened.
            //
            // We skip to the next data file to try and start from a clean slate.
            RecordStatus::FailedDeserialization(de) => {
                let reason = de.into_inner();
                error!(
                    ?reason,
                    "Last written record was unable to be deserialized. Corruption likely."
                );
                true
            }
        };

        // Reset our internal state, which closes the initial data file we opened, and mark
        // ourselves as needing to skip to the next data file.  This is a little convoluted, but we
        // need to ensure we follow the normal behavior of trying to open the next data file,
        // waiting for the reader to delete it if it already exists and hasn't been fully read yet,
        // etc.
        //
        // Essentially, we defer tthe actual skipping to avoid deadlocking here trying to open a
        // data file we might not be able to open yet.
        if should_skip_to_next_file {
            self.reset();
            self.mark_for_skip();
        }

        Ok(())
    }

    /// Ensures this writer is ready to attempt writer the next record.
    #[instrument(skip(self), level = "debug")]
    async fn ensure_ready_for_write(&mut self) -> io::Result<()> {
        // Check the overall size of the buffer and figure out if we can write.
        loop {
            // If we haven't yet exceeded the maximum buffer size, then we can proceed.  Otherwise,
            // wait for the reader to signal that they've made some progress.
            let total_buffer_size = self.ledger.get_total_buffer_size();
            let max_buffer_size = self.config.max_buffer_size;
            if total_buffer_size <= max_buffer_size {
                break;
            }

            trace!(
                total_buffer_size,
                max_buffer_size,
                "Buffer size limit reached. Waiting for reader progress."
            );

            self.ledger.wait_for_reader().await;
        }

        // If we already have an open writer, and we have no more space in the data file to write,
        // flush and close the file and mark ourselves as needing to open the _next_ data file.
        //
        // Likewise, if initialization detected an invalid record on the starting data file, and we
        // need to skip to the next file, we honor that here.
        let mut should_open_next = self.should_skip();
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
            self.flush_inner(true).await?;

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
            // this may be because it already exists and we're just picking up where we left off
            // from last time, but it could also be a data file that a reader hasn't completed yet.
            let data_file_path = if should_open_next {
                self.ledger.get_next_writer_data_file_path()
            } else {
                self.ledger.get_current_writer_data_file_path()
            };

            let maybe_data_file = OpenOptions::new()
                .append(true)
                .read(true)
                .create_new(true)
                .open(&data_file_path)
                .await;

            let file = match maybe_data_file {
                // We were able to create the file, so we're good to proceed.
                Ok(data_file) => Some((data_file, 0)),
                // We got back an error trying to open the file: might be that it already exists,
                // might be something else.
                Err(e) => match e.kind() {
                    ErrorKind::AlreadyExists => {
                        // We open the file again, without the atomic "create new" behavior.  If we
                        // can do that successfully, we check its length.  There's three main
                        // situations we encounter:
                        // - the reader may have deleted the data file between the atomic create
                        //   open and this one, and so we would expect the file length to be zero
                        // - the file still exists, and it's full: the reader may still be reading
                        //   it, or waiting for acknowledgements to be able to delete it
                        // - it may not be full, which could be because it's the data file the
                        //   writer left off on last time
                        let data_file = OpenOptions::new()
                            .append(true)
                            .read(true)
                            .create(true)
                            .open(&data_file_path)
                            .await?;
                        let metadata = data_file.metadata().await?;
                        let file_len = metadata.len();
                        if file_len == 0 || !should_open_next {
                            // The file is either empty, which means we created it and "own it" now,
                            // or it's not empty but we're not skipping to the next file, which can
                            // only mean that we're still initializing, and so this would be the
                            // data file we left off writing to.
                            Some((data_file, file_len))
                        } else {
                            // The file isn't empty, and we're not in initialization anymore, which
                            // means this data file is one that the reader still hasn't finished
                            // reading through yet, and so we must wait for the reader to delete it
                            // before we can proceed.
                            None
                        }
                    }
                    // Legitimate I/O error with the operation, bubble this up.
                    _ => return Err(e),
                },
            };

            if let Some((data_file, data_file_size)) = file {
                // We successfully opened the file and it can be written to.
                debug!(
                    data_file_path = data_file_path.to_string_lossy().as_ref(),
                    existing_file_size = data_file_size,
                    "Opened data file for writing."
                );

                // Make sure the file is flushed to disk, especially if we just created it.
                data_file.sync_all().await?;

                self.writer = Some(RecordWriter::new(
                    data_file,
                    data_file_size,
                    self.config.max_data_file_size,
                    self.config.max_record_size,
                ));
                self.data_file_size = data_file_size;

                // If we opened the "next" data file, we need to increment the current writer
                // file ID now to signal that the writer has moved on.
                if should_open_next {
                    self.ledger.state().increment_writer_file_id();
                    self.ledger.notify_writer_waiters();

                    debug!(
                        new_writer_file_id = self.ledger.get_current_writer_file_id(),
                        "Writer now on new data file."
                    );
                }

                return Ok(());
            }

            // The file is still present and waiting for a reader to finish reading it in order
            // to delete it.  Wait until the reader signals progress and try again.
            debug!("Target data file is still present and not yet processed. Waiting for reader.");
            self.ledger.wait_for_reader().await;
        }
    }

    /// Writes a record.
    ///
    /// If the record was written successfully, the number of bytes written to the data file will be
    /// returned.
    ///
    /// # Errors
    ///
    /// If an error occurred while writing the record, an error variant will be returned describing
    /// the error.
    #[instrument(skip_all, level = "trace")]
    pub async fn write_record(&mut self, mut record: T) -> Result<usize, WriterError<T>> {
        let record_events: NonZeroUsize = record
            .event_count()
            .try_into()
            .map_err(|_| WriterError::EmptyRecord)?;
        let record_events = record_events
            .get()
            .try_into()
            .expect("Vector does not support 128-bit platforms.");

        // Grab the next record ID and attempt to write the record.
        let record_id = self.ledger.state().get_next_writer_record_id();

        let bytes_written = loop {
            // Make sure we have an open data file to write to, which might also be us opening the
            // next data file because our first attempt at writing had to finalize a data file that
            // was already full.
            self.ensure_ready_for_write().await.context(IoSnafu)?;

            let writer = self
                .writer
                .as_mut()
                .expect("writer should exist after `ensure_ready_for_write`");
            match writer.write_record(record_id, record).await {
                Ok(n) => break n,
                Err(WriterError::DataFileFull {
                    record: old_record,
                    serialized_size,
                }) => {
                    // The data file is full, so we need to roll to the next one before attempting
                    // the write again.  We also recapture the record for the next write attempt.
                    self.mark_data_file_full();
                    record = old_record;

                    debug!(
                        current_data_file_size = self.data_file_size,
                        max_data_file_size = self.config.max_data_file_size,
                        last_attempted_write_size = serialized_size,
                        "Current data file reached maximum size. Rolling to the next data file."
                    );
                }
                Err(e) => return Err(e),
            }
        };

        // Since we succeeded in writing the record, increment the next record ID and metadata for
        // the writer.  We do this here to avoid consuming record IDs even if a write failed, as we
        // depend on the "record IDs are monotonic" invariant for detecting skipped records during read.
        self.ledger
            .state()
            .increment_next_writer_record_id(record_events);
        self.track_write(record_events, bytes_written as u64);

        trace!(
            record_id,
            record_events,
            bytes_written,
            data_file_id = self.ledger.get_current_writer_file_id(),
            "Wrote record."
        );

        Ok(bytes_written)
    }

    #[instrument(skip(self), level = "trace")]
    async fn flush_inner(&mut self, force_full_flush: bool) -> io::Result<()> {
        // We always flush the `BufWriter` when this is called, but we don't always flush to disk or
        // flush the ledger.  This is enough for readers on Linux since the file ends up in the page
        // cache, as we don't do any O_DIRECT fanciness, and the new contents can be immediately
        // read.
        //
        // TODO: Windows has a page cache as well, and macOS _should_, but we should verify this
        // behavior works on those platforms as well.
        if let Some(writer) = self.writer.as_mut() {
            writer.flush().await?;
            self.ledger.notify_writer_waiters();
        }

        if self.ledger.should_flush() || force_full_flush {
            if let Some(writer) = self.writer.as_mut() {
                writer.sync_all().await?;
            }

            self.ledger.flush()
        } else {
            Ok(())
        }
    }
    /// Flushes the writer.
    ///
    /// This must be called for the reader to be able to make progress.
    ///
    /// This does not ensure that the data is fully synchronized (i.e. `fsync`) to disk, however it
    /// may sometimes perform a full synchronization if the time since the last full synchronization
    /// occurred has exceeded a configured limit.
    ///
    /// # Errors
    ///
    /// If there is an error while flushing either the current data file or the ledger, an error
    /// variant will be returned describing the error.
    #[instrument(skip(self), level = "trace")]
    pub async fn flush(&mut self) -> io::Result<()> {
        self.flush_inner(false).await
    }
}

impl<T> Writer<T> {
    /// Closes this [`Writer`], marking it as done.
    ///
    /// Closing the writer signals to the reader that that no more records will be written until the
    /// buffer is reopened.  Writers and readers effectively share a "session", so until the writer
    /// and reader both close, the buffer cannot be reopened by another Vector instance.
    ///
    /// In turn, the reader is able to know that when the writer is marked as done, and it cannot
    /// read any more data, that nothing else is actually coming, and it can terminate by beginning
    /// to return `None`.
    #[instrument(skip(self), level = "trace")]
    pub fn close(&mut self) {
        if self.ledger.mark_writer_done() {
            debug!("Writer marked as closed.");
            self.ledger.notify_writer_waiters();
        }
    }
}

impl<T> Drop for Writer<T> {
    fn drop(&mut self) {
        self.close();
    }
}
