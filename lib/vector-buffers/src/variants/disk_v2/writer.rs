use std::{
    cmp::Ordering,
    convert::Infallible as StdInfallible,
    fmt,
    io::{self, ErrorKind},
    marker::PhantomData,
    num::NonZeroUsize,
    sync::Arc,
};

use bytes::BufMut;
use crc32fast::Hasher;
use rkyv::{
    AlignedVec, Infallible,
    ser::{
        Serializer,
        serializers::{
            AlignedSerializer, AllocScratch, AllocScratchError, BufferScratch, CompositeSerializer,
            CompositeSerializerError, FallbackScratch,
        },
    },
};
use snafu::{ResultExt, Snafu};
use tokio::io::{AsyncWrite, AsyncWriteExt};

use super::{
    common::{DiskBufferConfig, create_crc32c_hasher},
    io::Filesystem,
    ledger::Ledger,
    record::{Record, RecordStatus, validate_record_archive},
};

use crate::{
    Bufferable,
    encoding::{AsMetadata, Encodable},
    variants::disk_v2::{
        io::AsyncFile,
        reader::{RecordReader, decode_record_payload},
        record::{RECORD_HEADER_LEN, try_as_record_archive},
    },
};
use vector_common::finalization::{EventFinalizerGroups, EventStatus};

/// Error that occurred during calls to [`BufferWriter`].
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
    #[snafu(display("record too large: encoded length is {encoded_len}, limit is {limit}"))]
    RecordTooLarge { encoded_len: usize, limit: usize },

    /// The data file did not have enough remaining space to write the record.
    ///
    /// This could be because the data file is legitimately full, but is more commonly related to a
    /// record being big enough that it would exceed the max data file size.
    ///
    /// The record that was given to write is returned.
    #[snafu(display("data file full or record would exceed max data file size"))]
    DataFileFull { record: T, serialized_len: usize },

    /// A record reported that it contained more events than the number of bytes when encoded.
    ///
    /// This is nonsensical because we don't intend to ever support encoding zero-sized types
    /// through the buffer, and the logic we use to count the number of actual events in the buffer
    /// transitively depends on not being able to represent more than one event per encoded byte.
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
    /// error, and in fact, some encoders, it's the only possible error that can occur.
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
    /// Specifically, for `BufferWriter`, this can only ever be returned when creating the buffer, during
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

impl<T> WriterError<T>
where
    T: Bufferable,
{
    /// Whether this error means the record itself can never be written, no matter how many times
    /// it is retried — as opposed to a transient, environmental, or buffer-wide failure.
    ///
    /// This covers size violations: the record exceeds the maximum record size
    /// (`RecordTooLarge`), or the encoder bailed the moment it overflowed the size-limited buffer
    /// (`FailedToEncode`). Both are permanent regardless of retries.
    ///
    /// These records are dropped (finalizers resolved as `Delivered`) rather than retried forever
    /// or escalated into a fatal error that tears down the whole buffer/topology. All other errors
    /// — I/O errors, OOM scratch-space failures (`FailedToSerialize`), and writer-internal state
    /// errors (`InconsistentState`) — are explicitly excluded: they are either transient or
    /// writer-level faults, so they propagate as fatal errors.
    fn is_unwritable_record(&self) -> bool {
        matches!(
            self,
            Self::RecordTooLarge { .. } | Self::FailedToEncode { .. }
        )
    }
}

impl<T: Bufferable + PartialEq> PartialEq for WriterError<T> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Io { source: l_source }, Self::Io { source: r_source }) => {
                l_source.kind() == r_source.kind()
            }
            (
                Self::RecordTooLarge {
                    encoded_len: l_encoded_len,
                    limit: l_limit,
                },
                Self::RecordTooLarge {
                    encoded_len: r_encoded_len,
                    limit: r_limit,
                },
            ) => l_encoded_len == r_encoded_len && l_limit == r_limit,
            (
                Self::DataFileFull {
                    record: l_record,
                    serialized_len: l_serialized_len,
                },
                Self::DataFileFull {
                    record: r_record,
                    serialized_len: r_serialized_len,
                },
            ) => l_record == r_record && l_serialized_len == r_serialized_len,
            (
                Self::NonsensicalEventCount {
                    encoded_len: l_encoded_len,
                    event_count: l_event_count,
                },
                Self::NonsensicalEventCount {
                    encoded_len: r_encoded_len,
                    event_count: r_event_count,
                },
            ) => l_encoded_len == r_encoded_len && l_event_count == r_event_count,
            (
                Self::FailedToSerialize { reason: l_reason },
                Self::FailedToSerialize { reason: r_reason },
            )
            | (
                Self::FailedToValidate { reason: l_reason },
                Self::FailedToValidate { reason: r_reason },
            )
            | (
                Self::InconsistentState { reason: l_reason },
                Self::InconsistentState { reason: r_reason },
            ) => l_reason == r_reason,
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

impl<T> From<CompositeSerializerError<StdInfallible, AllocScratchError, StdInfallible>>
    for WriterError<T>
where
    T: Bufferable,
{
    fn from(e: CompositeSerializerError<StdInfallible, AllocScratchError, StdInfallible>) -> Self {
        match e {
            CompositeSerializerError::ScratchSpaceError(sse) => WriterError::FailedToSerialize {
                reason: format!("insufficient space to serialize encoded record: {sse}"),
            },
            // Only our scratch space strategy is fallible, so we should never get here.
            _ => unreachable!(),
        }
    }
}

impl<T> From<io::Error> for WriterError<T>
where
    T: Bufferable,
{
    fn from(source: io::Error) -> Self {
        WriterError::Io { source }
    }
}

/// The outcome of a [`BufferWriter::try_write_record`] call.
#[derive(Debug, PartialEq)]
pub enum TryWriteOutcome<T> {
    /// The record was written successfully and is now in the buffer.
    Written,
    /// The buffer is currently full; the record is returned for retry or discard.
    Full(T),
    /// The record permanently exceeded the maximum record size and was dropped.
    ///
    /// Its finalizers have already been resolved as [`EventStatus::Dropped`] (equivalent to
    /// `BatchStatus::Delivered`), so acking sources ack/checkpoint rather than redelivering
    /// a record that can never be written.
    Dropped,
}

/// RAII guard that resolves finalizers as [`EventStatus::Errored`] on drop unless explicitly
/// disarmed.
///
/// Used in `try_write_record_inner` so that every `?` exit automatically notifies acking sources
/// to nack / withhold checkpoints for any record that did not reach durable storage.
struct FinalizerGuard {
    finalizers: EventFinalizerGroups,
    error_on_drop: bool,
}

impl FinalizerGuard {
    fn new(finalizers: EventFinalizerGroups) -> Self {
        Self {
            finalizers,
            error_on_drop: true,
        }
    }

    /// Releases the guard without marking finalizers as errored.
    ///
    /// Call when the record was intentionally dropped (unwritable) or successfully flushed to
    /// disk — both cases where the upstream source should ack rather than retry.
    fn disarm(mut self) {
        self.error_on_drop = false;
    }

    /// Returns the finalizers for reattachment to the recovered record on buffer-full retry.
    fn into_inner(mut self) -> EventFinalizerGroups {
        self.error_on_drop = false;
        std::mem::take(&mut self.finalizers)
    }
}

impl Drop for FinalizerGuard {
    fn drop(&mut self) {
        if self.error_on_drop {
            self.finalizers.update_status(EventStatus::Errored);
        }
    }
}

#[derive(Debug)]
pub(super) struct WriteToken {
    event_count: usize,
    serialized_len: usize,
}

impl WriteToken {
    pub fn event_count(&self) -> usize {
        self.event_count
    }

    pub fn serialized_len(&self) -> usize {
        self.serialized_len
    }
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct FlushResult {
    pub events_flushed: u64,
    pub bytes_flushed: u64,
}

/// Wraps an [`AsyncWrite`] value and buffers individual writes, while signalling implicit flushes.
///
/// As the [`BufferWriter`] must track when writes have theoretically made it to disk, we care about
/// situations where the internal write buffer for a data file has been flushed to make room.  In
/// order to provide this information, we track the number of events represented by a record when
/// writing its serialized form.
///
/// If an implicit buffer flush must be performed before a write can complete, or a manual flush is
/// requested, we return this information to the caller, letting them know how many events, and how
/// many bytes, were flushed.
struct TrackingBufWriter<W> {
    inner: W,
    buf: Vec<u8>,
    unflushed_events: usize,
}

impl<W: AsyncWrite + Unpin> TrackingBufWriter<W> {
    /// Creates a new `TrackingBufWriter` with the specified buffer capacity.
    fn with_capacity(cap: usize, inner: W) -> Self {
        Self {
            inner,
            buf: Vec::with_capacity(cap),
            unflushed_events: 0,
        }
    }

    /// Writes the given buffer.
    ///
    /// If enough internal buffer capacity is available, then this write will be buffered internally
    /// until [`flush`] is called.  If there's not enough remaining internal buffer capacity, then
    /// the internal buffer will be flushed to the inner writer first.  If the given buffer is
    /// larger than the internal buffer capacity, then it will be written directly to the inner
    /// writer.
    ///
    /// Internally, a counter is kept of how many buffered events are waiting to be flushed. This
    /// count is incremented every time `write` can fully buffer the record without having to flush
    /// to the inner writer.
    ///
    /// If this call requires the internal buffer to be flushed out to the inner writer, then the
    /// write result will indicate how many buffered events were flushed, and their total size in
    /// bytes.  Additionally, if the given buffer is larger than the internal buffer itself, it will
    /// also be included in the write result as well.
    ///
    /// # Errors
    ///
    /// If a write to the inner writer occurs, and that write encounters an error, an error variant
    /// will be returned describing the error.
    async fn write(&mut self, event_count: usize, buf: &[u8]) -> io::Result<Option<FlushResult>> {
        let mut flush_result = None;

        // If this write would cause us to exceed our internal buffer capacity, flush whatever we
        // have buffered already.
        if self.buf.len() + buf.len() > self.buf.capacity() {
            flush_result = self.flush().await?;
        }

        // If the given buffer is too large to be buffered at all, then bypass the internal buffer.
        if buf.len() >= self.buf.capacity() {
            self.inner.write_all(buf).await?;

            let flush_result = flush_result.get_or_insert(FlushResult::default());
            flush_result.events_flushed += event_count as u64;
            flush_result.bytes_flushed += buf.len() as u64;
        } else {
            self.buf.extend_from_slice(buf);
            self.unflushed_events += event_count;
        }

        Ok(flush_result)
    }

    /// Flushes the internal buffer to the underlying writer.
    ///
    /// Internally, a counter is kept of how many buffered events are waiting to be flushed. This
    /// count is incremented every time `write` can fully buffer the record without having to flush
    /// to the inner writer.
    ///
    /// If any buffered record are present, then the write result will indicate how many
    /// individual events were flushed, including their total size in bytes.
    ///
    /// # Errors
    ///
    /// If a write to the underlying writer occurs, and that write encounters an error, an error variant
    /// will be returned describing the error.
    async fn flush(&mut self) -> io::Result<Option<FlushResult>> {
        if self.buf.is_empty() {
            return Ok(None);
        }

        let events_flushed = self.unflushed_events as u64;
        let bytes_flushed = self.buf.len() as u64;

        let result = self.inner.write_all(&self.buf[..]).await;
        self.unflushed_events = 0;
        self.buf.clear();

        result.map(|()| {
            Some(FlushResult {
                events_flushed,
                bytes_flushed,
            })
        })
    }

    /// Gets a reference to the underlying writer.
    #[cfg(test)]
    fn get_ref(&self) -> &W {
        &self.inner
    }

    /// Gets a mutable reference to the underlying writer.
    fn get_mut(&mut self) -> &mut W {
        &mut self.inner
    }
}

impl<W: fmt::Debug> fmt::Debug for TrackingBufWriter<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TrackingBufWriter")
            .field("writer", &self.inner)
            .field(
                "buffer",
                &format_args!("{}/{}", self.buf.len(), self.buf.capacity()),
            )
            .field("unflushed_events", &self.unflushed_events)
            .finish()
    }
}

/// Buffered writer that handles encoding, checksumming, and serialization of records.
#[derive(Debug)]
pub(super) struct RecordWriter<W, T> {
    writer: TrackingBufWriter<W>,
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
    W: AsyncFile + Unpin,
    T: Bufferable,
{
    /// Creates a new [`RecordWriter`] around the provided writer.
    ///
    /// Internally, the writer is wrapped in a [`BufWriter`], so callers should not pass in an
    /// already buffered writer.
    pub fn new(
        writer: W,
        current_data_file_size: u64,
        write_buffer_size: usize,
        max_data_file_size: u64,
        max_record_size: usize,
    ) -> Self {
        // These should also be getting checked at a higher level, but we're double-checking them here to be absolutely sure.
        let max_record_size_converted = u64::try_from(max_record_size)
            .expect("Maximum record size must be less than 2^64 bytes.");

        debug_assert!(
            max_record_size > RECORD_HEADER_LEN,
            "maximum record length must be larger than size of record header itself"
        );
        debug_assert!(
            max_data_file_size >= max_record_size_converted,
            "must always be able to fit at least one record into a data file"
        );

        // We subtract the length of the record header from our allowed maximum record size, because we have to make sure
        // that when we go to actually wrap and serialize the encoded record, we're limiting the actual bytes we write
        // to disk to within `max_record_size`.
        //
        // This could lead to us reducing the encode buffer size limit by slightly more than necessary, since
        // `RECORD_HEADER_LEN` might be overaligned compared to what it would be necessary when we look at the
        // encoded/serialized record... but that's OK, but it's only going to differ by 8 bytes at most.
        let max_record_size = max_record_size - RECORD_HEADER_LEN;

        Self {
            writer: TrackingBufWriter::with_capacity(write_buffer_size, writer),
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

    async fn truncate(&mut self, size: u64) -> io::Result<()> {
        if size > self.current_data_file_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot extend a file through the truncation API",
            ));
        }

        self.writer.flush().await?;
        let writer = self.writer.get_mut();
        writer.truncate(size).await?;
        writer.sync_all().await?;
        self.current_data_file_size = size;

        Ok(())
    }

    /// Gets a reference to the underlying writer.
    #[cfg(test)]
    pub fn get_ref(&self) -> &W {
        self.writer.get_ref()
    }

    /// Whether or not `amount` bytes could be written while obeying the data file size limit.
    ///
    /// If no bytes have written at all to a data file, then `amount` is allowed to exceed the
    /// limit, otherwise a record would never be able to be written.
    fn can_write(&self, amount: usize) -> bool {
        let amount = u64::try_from(amount).expect("`amount` should need ever 2^64 bytes.");

        self.current_data_file_size + amount <= self.max_data_file_size
    }

    /// Archives a record.
    ///
    /// This encodes the record, as well as serializes it into its archival format that will be
    /// stored on disk.  The total size of the archived record, including the length delimiter
    /// inserted before the archived record, will be returned.
    ///
    /// # Errors
    ///
    /// Errors can occur during the encoding or serialization stage.  If an error occurs
    /// during any of these stages, an appropriate error variant will be returned describing the error.
    #[instrument(skip(self, record), level = "trace")]
    pub fn archive_record(&mut self, id: u64, record: T) -> Result<WriteToken, WriterError<T>> {
        let event_count = record.event_count();

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
            .map(|()| self.encode_buf.len())
            .context(FailedToEncodeSnafu)?;
        if encoded_len > self.max_record_size {
            return Err(WriterError::RecordTooLarge {
                encoded_len,
                limit: self.max_record_size,
            });
        }

        let metadata = T::get_metadata().into_u32();
        let wrapped_record =
            Record::with_checksum(id, metadata, &self.encode_buf, &self.checksummer);

        // Push 8 dummy bytes where our length delimiter will sit.  We'll fix this up after
        // serialization.  Notably, `AlignedSerializer` will report the serializer position as
        // the length of its backing store, which now includes our 8 bytes, so we _subtract_
        // those from the position when figuring out the actual value to write back after.
        //
        // We write it this way -- in the serializer buffer, and not as a separate write -- so that
        // we can do a single write but also so that we always have an aligned buffer.
        self.ser_buf
            .extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

        // Now serialize the record, which puts it into its archived form.  This is what powers our
        // ability to do zero-copy deserialization from disk.
        let mut serializer = CompositeSerializer::new(
            AlignedSerializer::new(&mut self.ser_buf),
            FallbackScratch::new(
                BufferScratch::new(&mut self.ser_scratch),
                AllocScratch::new(),
            ),
            Infallible,
        );

        let serialized_len = serializer
            .serialize_value(&wrapped_record)
            .map(|_| serializer.pos())?;

        // Sanity check before we do our length math.
        if serialized_len <= 8 || self.ser_buf.len() != serialized_len {
            return Err(WriterError::InconsistentState {
                reason: format!(
                    "serializer position invalid after serializing record: pos={} len={}",
                    serialized_len,
                    self.ser_buf.len(),
                ),
            });
        }

        // With the record archived and serialized, do our final check to ensure we can fit this
        // write.  We're doing this earlier than the actual call to flush it because it gives us
        // a chance to hand back the event so that the caller can roll to a new data file first
        // before attempting the writer again.
        if !self.can_write(serialized_len) {
            debug!(
                current_data_file_size = self.current_data_file_size,
                max_data_file_size = self.max_data_file_size,
                archive_on_disk_len = serialized_len,
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
                serialized_len,
            });
        }

        // Fix up our length delimiter.
        let archive_len = serialized_len - 8;
        let wire_archive_len: u64 = archive_len
            .try_into()
            .expect("archive len should always fit into a u64");
        let archive_len_buf = wire_archive_len.to_be_bytes();

        let length_delimiter_dst = &mut self.ser_buf[0..8];
        length_delimiter_dst.copy_from_slice(&archive_len_buf[..]);

        Ok(WriteToken {
            event_count,
            serialized_len,
        })
    }

    /// Writes a record.
    ///
    /// If the write is successful, the number of bytes written to the buffer are returned.
    /// Additionally, if any internal buffers required an implicit flush, the result of that flush
    /// operation is returned as well.
    ///
    /// As we internally buffers write to the underlying data file, to reduce the number of syscalls
    /// required to pushed serialized records to the data file, we sometimes will write a record
    /// which would overflow the internal buffer.  Doing so means we have to first flush the buffer
    /// before continuing with buffering the current write.  As some invariants are based on knowing
    /// when a record has actually been written to the data file, we return any information of
    /// implicit flushes so that the writer can be aware of when data has actually made it to the
    /// data file or not.
    ///
    /// # Errors
    ///
    /// Errors can occur during the encoding, serialization, or I/O stage.  If an error occurs
    /// during any of these stages, an appropriate error variant will be returned describing the error.
    #[instrument(skip(self, record), level = "trace")]
    #[cfg(test)]
    pub async fn write_record(
        &mut self,
        id: u64,
        record: T,
    ) -> Result<(usize, Option<FlushResult>), WriterError<T>> {
        let token = self.archive_record(id, record)?;
        self.flush_record(token).await
    }

    /// Flushes the previously-archived record.
    ///
    /// If the flush is successful, the number of bytes written to the buffer are returned.
    /// Additionally, if any internal buffers required an implicit flush, the result of that flush
    /// operation is returned as well.
    ///
    /// As we internally buffers write to the underlying data file, to reduce the number of syscalls
    /// required to pushed serialized records to the data file, we sometimes will write a record
    /// which would overflow the internal buffer.  Doing so means we have to first flush the buffer
    /// before continuing with buffering the current write.  As some invariants are based on knowing
    /// when a record has actually been written to the data file, we return any information of
    /// implicit flushes so that the writer can be aware of when data has actually made it to the
    /// data file or not.
    #[instrument(skip(self), level = "trace")]
    pub async fn flush_record(
        &mut self,
        token: WriteToken,
    ) -> Result<(usize, Option<FlushResult>), WriterError<T>> {
        // Make sure the write token we've been given matches whatever the last call to `archive_record` generated.
        let event_count = token.event_count();
        let serialized_len = token.serialized_len();
        debug_assert_eq!(
            serialized_len,
            self.ser_buf.len(),
            "using write token from non-contiguous archival call"
        );

        let flush_result = self
            .writer
            .write(event_count, &self.ser_buf[..])
            .await
            .context(IoSnafu)?;

        // Update our current data file size.
        self.current_data_file_size += u64::try_from(serialized_len)
            .expect("Serialized length of record should never exceed 2^64 bytes.");

        // `archive_record` only hands back a flushable token after `can_write`
        // confirmed this record fits the remaining room in the file, so the file
        // never grows past its limit and a record never spans two files. A size
        // counter that drifted past the limit here means the gate was fed a wrong
        // on-disk size and a record was written across the boundary.
        #[cfg(feature = "antithesis-disk-asserts")]
        {
            #![allow(clippy::disallowed_types)] // once_cell::Lazy
            antithesis_sdk::assert_always_or_unreachable!(
                self.current_data_file_size <= self.max_data_file_size,
                "a record never spans two data files",
                &serde_json::json!({
                    "current_data_file_size": self.current_data_file_size,
                    "max_data_file_size": self.max_data_file_size,
                    "serialized_len": serialized_len,
                })
            );
        }

        Ok((serialized_len, flush_result))
    }

    /// Recovers an archived record that has not yet been flushed.
    ///
    /// In some cases, we must archive a record to see how large the resulting archived record is, and potentially
    /// recover the original record if it's too large, and so on.
    ///
    /// This method allows decoding an archived record that is still sitting in the internal buffers waiting to be
    /// flushed. Technically, this decodes the original record back from its archived/encoded form, and so this isn't a
    /// clone but it does mean incurring the cost of decoding directly.
    ///
    /// # Errors
    ///
    /// If the archived record cannot be deserialized from its archival form, or can't be decoded back to its original
    /// form `T`, an error variant will be returned describing the error. Notably, the only error we return is
    /// `InconsistentState`, as being unable to immediately deserialize and decode a record we just serialized and
    /// encoded implies a fatal, and unrecoverable, error with the buffer implementation as a whole.
    #[instrument(skip(self), level = "trace")]
    pub fn recover_archived_record(&mut self, token: &WriteToken) -> Result<T, WriterError<T>> {
        // Make sure the write token we've been given matches whatever the last call to `archive_record` generated.
        let serialized_len = token.serialized_len();
        debug_assert_eq!(
            serialized_len,
            self.ser_buf.len(),
            "using write token from non-contiguous archival call"
        );

        // First, decode the archival wrapper. This means skipping the length delimiter.
        let wrapped_record = try_as_record_archive(&self.ser_buf[8..]).map_err(|_| {
            WriterError::InconsistentState {
                reason: "failed to decode archived record immediately after archiving it"
                    .to_string(),
            }
        })?;

        // Now we can actually decode it as `T`.
        let record_metadata = T::Metadata::from_u32(wrapped_record.metadata()).ok_or(
            WriterError::InconsistentState {
                reason: "failed to decode record metadata immediately after encoding it"
                    .to_string(),
            },
        )?;

        T::decode(record_metadata, wrapped_record.payload()).map_err(|_| {
            WriterError::InconsistentState {
                reason: "failed to decode record immediately after encoding it".to_string(),
            }
        })
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
    pub async fn flush(&mut self) -> io::Result<Option<FlushResult>> {
        self.writer.flush().await
    }

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
pub struct BufferWriter<T, FS>
where
    FS: Filesystem,
    FS::File: Unpin,
{
    ledger: Arc<Ledger<FS>>,
    config: DiskBufferConfig<FS>,
    writer: Option<RecordWriter<FS::File, T>>,
    next_record_id: u64,
    unflushed_events: u64,
    data_file_size: u64,
    unflushed_bytes: u64,
    data_file_full: bool,
    skip_to_next: bool,
    ready_to_write: bool,
    _t: PhantomData<T>,
}

impl<T, FS> BufferWriter<T, FS>
where
    T: Bufferable,
    FS: Filesystem + fmt::Debug + Clone,
    FS::File: Unpin,
{
    /// Creates a new [`BufferWriter`] attached to the given [`Ledger`].
    pub(crate) fn new(ledger: Arc<Ledger<FS>>) -> Self {
        let config = ledger.config().clone();
        let next_record_id = ledger.state().get_next_writer_record_id();
        BufferWriter {
            ledger,
            config,
            writer: None,
            data_file_size: 0,
            data_file_full: false,
            unflushed_bytes: 0,
            skip_to_next: false,
            ready_to_write: false,
            next_record_id,
            unflushed_events: 0,
            _t: PhantomData,
        }
    }

    fn get_next_record_id(&mut self) -> u64 {
        self.next_record_id + self.unflushed_events
    }

    fn track_write(&mut self, event_count: usize, record_size: u64) {
        self.data_file_size += record_size;
        self.unflushed_events += event_count as u64;
        self.unflushed_bytes += record_size;
    }

    fn publish_flushed_progress(&mut self, flushed_events: u64, flushed_bytes: u64) {
        debug_assert!(
            flushed_events <= self.unflushed_events,
            "tried to flush more events than are currently unflushed"
        );
        debug_assert!(
            flushed_bytes <= self.unflushed_bytes,
            "tried to flush more bytes than are currently unflushed"
        );

        self.unflushed_events -= flushed_events;
        self.unflushed_bytes -= flushed_bytes;
        self.next_record_id = self
            .ledger
            .publish_writer_progress(flushed_events, flushed_bytes);
    }

    fn can_write(&self) -> bool {
        !self.data_file_full && self.data_file_size < self.config.max_data_file_size
    }

    fn can_write_record(&self, amount: usize) -> bool {
        let total_buffer_size = self.ledger.get_total_buffer_size() + self.unflushed_bytes;
        let potential_write_len =
            u64::try_from(amount).expect("Vector only supports 64-bit architectures.");

        self.can_write() && total_buffer_size + potential_write_len <= self.config.max_buffer_size
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

    async fn truncate_current_data_file_to_checkpoint(&mut self) -> Result<(), WriterError<T>> {
        let checkpoint_next_record_id = self.ledger.state().get_next_writer_record_id();
        let data_file_path = self.ledger.get_current_writer_data_file_path();
        let data_file = self
            .ledger
            .filesystem()
            .open_file_readable(&data_file_path)
            .await
            .context(IoSnafu)?;
        let mut reader = RecordReader::new(data_file);
        let mut record_start_offset = 0;

        loop {
            let token = match reader.try_next_record(true).await {
                Ok(Some(token)) => token,
                Ok(None) => break,
                Err(e) if e.is_bad_read() => {
                    warn!(
                        data_file_path = data_file_path.to_string_lossy().as_ref(),
                        truncate_at = record_start_offset,
                        error = %e,
                        "Truncating torn writer data file tail at last checkpointed record boundary."
                    );
                    break;
                }
                Err(e) => {
                    return Err(WriterError::FailedToValidate {
                        reason: e.to_string(),
                    });
                }
            };

            let record_id = token.record_id();
            let record_bytes = token.record_bytes() as u64;
            if record_id >= checkpoint_next_record_id {
                debug!(
                    data_file_path = data_file_path.to_string_lossy().as_ref(),
                    checkpoint_next_record_id,
                    record_id,
                    truncate_at = record_start_offset,
                    "Truncating writer data file tail beyond durable checkpoint."
                );
                break;
            }

            let record: T =
                reader
                    .read_record(token)
                    .map_err(|e| WriterError::FailedToValidate {
                        reason: e.to_string(),
                    })?;
            let record_events =
                u64::try_from(record.event_count()).expect("event count should never exceed u64");
            let record_next = record_id + record_events;
            if record_next > checkpoint_next_record_id {
                warn!(
                    data_file_path = data_file_path.to_string_lossy().as_ref(),
                    checkpoint_next_record_id,
                    record_id,
                    record_events,
                    truncate_at = record_start_offset,
                    "Truncating writer data file at record that crosses durable checkpoint."
                );
                break;
            }

            record_start_offset += record_bytes;
        }

        self.truncate_current_data_file(record_start_offset).await?;
        Ok(())
    }

    async fn truncate_current_data_file(&mut self, size: u64) -> io::Result<()> {
        let writer = self
            .writer
            .as_mut()
            .expect("writer should exist after `ensure_ready_for_write`");
        writer.truncate(size).await?;
        self.data_file_size = size;

        Ok(())
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
        // We don't try validating again after doing so initially.
        if self.ready_to_write {
            warn!("Writer already initialized.");
            return Ok(());
        }

        debug!(
            current_writer_data_file = ?self.ledger.get_current_writer_data_file_path(),
            "Validating last written record in current data file."
        );
        self.ensure_ready_for_write().await.context(IoSnafu)?;

        // If our current file is empty, there's no sense doing this check.
        if self.data_file_size == 0 {
            self.ready_to_write = true;
            return Ok(());
        }

        // We do a neat little trick here where we open an immutable memory-mapped region against our
        // current writer data file, which lets us treat it as one big buffer... which is useful for
        // asking `rkyv` to deserialize just the last record from the file, without having to seek
        // directly to the start of the record where the length delimiter is.
        let data_file_path = self.ledger.get_current_writer_data_file_path();
        let data_file_mmap = self
            .ledger
            .filesystem()
            .open_mmap_readable(&data_file_path)
            .await
            .context(IoSnafu)?;

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
                let record_next = last_record_id + record_events;

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
                            ledger_next,
                            last_record_id,
                            record_events,
                            "Last record written to data file is behind expected position. Events have likely been lost."
                        );
                        true
                    }
                    Ordering::Less => {
                        // The data file is ahead of the durable writer checkpoint. Treat the
                        // checkpoint as authoritative: truncate any post-checkpoint tail instead
                        // of fast-forwarding the ledger to match bytes that may not have been
                        // durably committed before the crash.
                        debug!(
                            ledger_next,
                            last_record_id,
                            record_events,
                            record_next,
                            "Writer data file is ahead of durable checkpoint."
                        );
                        self.truncate_current_data_file_to_checkpoint().await?;
                        self.unflushed_events = 0;

                        false
                    }
                }
            }
            // The record payload was corrupted, somehow: we know the checksum failed to match on
            // both sides, but we do not know whether the damage is limited to the last record or
            // extends farther back. Treat the durable checkpoint as authoritative and truncate the
            // current file to the last checkpointed boundary we can validate.
            RecordStatus::Corrupted { .. } => {
                error!(
                    "Last written record did not match the expected checksum. Corruption likely."
                );
                self.truncate_current_data_file_to_checkpoint().await?;
                false
            }
            // The record itself was corrupted, somehow: it was sufficiently different that `rkyv`
            // couldn't even validate it, which likely means missing bytes but could also be certain
            // bytes being invalid for the struct fields they represent. Like invalid checksums,
            // truncate the current file back to the durable checkpoint instead of rolling forward
            // onto a new data file.
            RecordStatus::FailedDeserialization(de) => {
                let reason = de.into_inner();
                error!(
                    ?reason,
                    "Last written record was unable to be deserialized. Corruption likely."
                );
                self.truncate_current_data_file_to_checkpoint().await?;
                false
            }
        };

        // Reset our internal state, which closes the initial data file we opened, and mark
        // ourselves as needing to skip to the next data file.  This is a little convoluted, but we
        // need to ensure we follow the normal behavior of trying to open the next data file,
        // waiting for the reader to delete it if it already exists and hasn't been fully read yet,
        // etc.
        //
        // Essentially, we defer the actual skipping to avoid deadlocking here trying to open a
        // data file we might not be able to open yet.
        if should_skip_to_next_file {
            self.reset();
            self.mark_for_skip();
        }

        self.ready_to_write = true;

        Ok(())
    }

    /// Moves the writer to an empty reader checkpoint when recovery proves the reader is one file
    /// ahead and no unread bytes remain in the checkpoint window.
    ///
    /// The same file-ID relationship can represent a completely full wrapped window, so the zero
    /// unread-byte check is required before treating it as the reader-ahead recovery state.
    pub(super) async fn align_with_reader_ahead_checkpoint(
        &mut self,
        unread_buffer_size: u64,
    ) -> Result<bool, WriterError<T>> {
        let reader_file_id = self.ledger.get_current_reader_file_id();
        let writer_file_id = self.ledger.get_current_writer_file_id();
        if unread_buffer_size != 0 || reader_file_id != self.ledger.get_next_writer_file_id() {
            return Ok(false);
        }

        let reader_data_file_path = self.ledger.get_current_reader_data_file_path();
        let reader_data_file = self
            .ledger
            .filesystem()
            .open_file_writable(&reader_data_file_path)
            .await
            .context(IoSnafu)?;
        reader_data_file.truncate(0).await.context(IoSnafu)?;
        reader_data_file.sync_all().await.context(IoSnafu)?;
        drop(reader_data_file);

        self.reset();
        self.mark_for_skip();
        self.ensure_ready_for_write().await.context(IoSnafu)?;
        self.ledger.flush().context(IoSnafu)?;

        debug!(
            previous_writer_file_id = writer_file_id,
            reader_file_id, "Advanced writer to empty reader-ahead checkpoint."
        );

        Ok(true)
    }

    fn is_buffer_full(&self) -> bool {
        let total_buffer_size = self.ledger.get_total_buffer_size() + self.unflushed_bytes;
        let max_buffer_size = self.config.max_buffer_size;
        total_buffer_size >= max_buffer_size
    }

    /// Ensures this writer is ready to attempt writer the next record.
    #[instrument(skip(self), level = "debug")]
    // The inline antithesis assertion block pushes this over the line limit. Its
    // source lines count even when the feature is off, so the allow is unconditional.
    #[allow(clippy::too_many_lines)]
    async fn ensure_ready_for_write(&mut self) -> io::Result<()> {
        // Check the overall size of the buffer and figure out if we can write.
        loop {
            // If we haven't yet exceeded the maximum buffer size, then we can proceed. Likewise, if
            // we're still validating our last write, then we know it doesn't matter if the buffer
            // is full or not because we're not doing any actual writing here.
            //
            // Otherwise, wait for the reader to signal that they've made some progress.
            if !self.is_buffer_full() || !self.ready_to_write {
                break;
            }

            trace!(
                total_buffer_size = self.ledger.get_total_buffer_size() + self.unflushed_bytes,
                max_buffer_size = self.config.max_buffer_size,
                "Buffer size limit reached. Waiting for reader progress."
            );

            // The writer is now blocked on a full buffer, the precondition for the
            // backpressure path and for the underflow that can wedge it forever.
            #[cfg(feature = "antithesis-disk-asserts")]
            {
                #![allow(clippy::disallowed_types)] // once_cell::Lazy
                antithesis_sdk::assert_sometimes!(
                    true,
                    "the writer blocks on a full buffer",
                    &serde_json::json!({
                        "total_buffer_size": self.ledger.get_total_buffer_size() + self.unflushed_bytes,
                        "max_buffer_size": self.config.max_buffer_size,
                    })
                );
            }

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
            // logically complete files once they have read and acknowledged them, and cleanup then
            // deletes stale files in the background. Our first loop iteration is the happy path,
            // trying to create the new file. If we can't create it,
            // this may be because it already exists and we're just picking up where we left off
            // from last time, but it could also be a data file that a reader hasn't completed yet.
            let cleanup_guard = if should_open_next {
                Some(self.ledger.lock_data_file_cleanup().await)
            } else {
                None
            };
            let data_file_path = if should_open_next {
                self.ledger.get_next_writer_data_file_path()
            } else {
                self.ledger.get_current_writer_data_file_path()
            };

            let maybe_data_file = self
                .ledger
                .filesystem()
                .open_file_writable_atomic(&data_file_path)
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
                        let data_file = self
                            .ledger
                            .filesystem()
                            .open_file_writable(&data_file_path)
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
                    self.config.write_buffer_size,
                    self.config.max_data_file_size,
                    self.config.max_record_size,
                ));
                self.data_file_size = data_file_size;

                // If we opened the "next" data file, we need to increment the current writer
                // file ID now to signal that the writer has moved on.
                if should_open_next {
                    self.ledger.state().increment_writer_file_id();
                    self.ledger.notify_writer_waiters();

                    // The writer just rolled to a fresh data file, the boundary the
                    // crash, partial-write, and file-id-rollover faults act on.
                    #[cfg(feature = "antithesis-disk-asserts")]
                    {
                        #![allow(clippy::disallowed_types)] // once_cell::Lazy
                        antithesis_sdk::assert_sometimes!(
                            true,
                            "the writer rolls to a new data file",
                            &serde_json::json!({
                                "new_writer_file_id": self.ledger.get_current_writer_file_id(),
                            })
                        );
                    }

                    debug!(
                        new_writer_file_id = self.ledger.get_current_writer_file_id(),
                        "Writer now on new data file."
                    );
                }

                return Ok(());
            }

            // The file is still present and waiting for a reader to finish reading it in order
            // for cleanup to delete it. Release the cleanup guard first so the background cleaner
            // can make that progress.
            drop(cleanup_guard);

            // Wait until the reader signals progress and try again.
            debug!("Target data file is still present and not yet processed. Waiting for reader.");
            self.ledger.wait_for_reader().await;
        }
    }

    /// Attempts to write a record.
    ///
    /// Returns a [`TryWriteOutcome`] indicating whether the record was written, the buffer was
    /// full (record returned for retry), or the record was permanently dropped because it exceeded
    /// the maximum record size.
    ///
    /// # Errors
    ///
    /// If an error occurred while writing the record, an error variant will be returned describing
    /// the error.
    pub async fn try_write_record(
        &mut self,
        record: T,
    ) -> Result<TryWriteOutcome<T>, WriterError<T>> {
        self.try_write_record_inner(record)
            .await
            .map(|inner| match inner {
                // A zero-byte write is the sentinel for a silently dropped oversized record.
                Ok(0) => TryWriteOutcome::Dropped,
                Ok(_) => TryWriteOutcome::Written,
                Err(record) => TryWriteOutcome::Full(record),
            })
    }

    #[instrument(skip_all, level = "debug")]
    async fn try_write_record_inner(
        &mut self,
        mut record: T,
    ) -> Result<Result<usize, T>, WriterError<T>> {
        // If the buffer is already full, we definitely can't complete this write.
        if self.is_buffer_full() {
            return Ok(Err(record));
        }

        let record_events: NonZeroUsize = record
            .event_count()
            .try_into()
            .map_err(|_| WriterError::EmptyRecord)?;

        // Extract the finalizers before `archive_record` consumes the record. The encoder
        // unconditionally consumes the record (even on failure), so we must take what we need here
        // to handle the case where encoding fails because the record is too large to ever write.
        // The guard automatically resolves the finalizers as Errored if this function exits via
        // `?`; call `disarm()` or `into_inner()` on the non-error paths.
        let record_finalizers = FinalizerGuard::new(record.take_finalizer_groups());

        // Grab the next record ID and attempt to write the record.
        let record_id = self.get_next_record_id();

        let token = loop {
            // Make sure we have an open data file to write to, which might also be us opening the
            // next data file because our first attempt at writing had to finalize a data file that
            // was already full.
            self.ensure_ready_for_write().await.context(IoSnafu)?;

            let writer = self
                .writer
                .as_mut()
                .expect("writer should exist after `ensure_ready_for_write`");

            // Archive the record, which if it succeeds in terms of encoding, etc, will give us a token that we can use
            // to eventually write it to storage. This may fail if the record writer detects it can't fit the archived
            // record in the current data file, so we handle that separately. All other errors must be handled by the caller.
            match writer.archive_record(record_id, record) {
                Ok(token) => break token,
                Err(we) => match we {
                    WriterError::DataFileFull {
                        record: old_record,
                        serialized_len,
                    } => {
                        // The data file is full, so we need to roll to the next one before attempting
                        // the write again.  We also recapture the record for the next write attempt.
                        self.mark_data_file_full();
                        record = old_record;

                        debug!(
                            current_data_file_size = self.data_file_size,
                            max_data_file_size = self.config.max_data_file_size,
                            last_attempted_write_size = serialized_len,
                            "Current data file reached maximum size. Rolling to the next data file."
                        );
                    }
                    e if e.is_unwritable_record() => {
                        // The record can never be written regardless of retries — either it
                        // exceeds the maximum record size or the rkyv wrapper serialization
                        // failed permanently. Retrying would loop forever and propagating the
                        // error would tear down the entire buffer/topology. Instead, drop just
                        // this record and carry on: the buffer and every other record are unharmed.
                        //
                        // Drop the finalizers with their default EventStatus::Dropped, which
                        // propagates as BatchStatus::Delivered. Acking sources therefore ack/checkpoint
                        // the record rather than nacking or stalling, preventing a permanent
                        // failure from becoming a retry loop. The ledger records matching
                        // received and dropped usage so occupancy stays balanced.
                        //
                        // `RecordTooLarge` carries the exact encoded length; `FailedToEncode` bails
                        // before that size is known, so we fall back to the configured limit as a
                        // lower-bound estimate. This byte size only feeds the cumulative
                        // received/discarded byte counters — occupancy self-cancels because the
                        // ledger adds the same value to both — so an estimate is acceptable for the
                        // rare encode-failure case, and it avoids an `O(events)` `size_of()` on the
                        // hot write path just to report a value consumed only when a record drops.
                        let encoded_len = match &e {
                            WriterError::RecordTooLarge { encoded_len, .. } => *encoded_len,
                            _ => self.config.max_record_size,
                        };
                        error!(
                            message = "Record cannot be written to the disk buffer; dropping it.",
                            event_count = record_events.get(),
                            encoded_len,
                            max_record_size = self.config.max_record_size,
                            error = %e,
                        );
                        record_finalizers.disarm();
                        self.ledger.track_unwritable_dropped_record(
                            record_events.get() as u64,
                            encoded_len as u64,
                        );
                        return Ok(Ok(0));
                    }
                    e => return Err(e),
                },
            }
        };

        // Now that we know the record was archived successfully -- record wasn't too large, etc -- we actually need
        // to check if it will fit based on our current buffer size. If not, we recover the record from the writer's
        // internal buffers, as we haven't yet flushed it, and we return it to the caller.
        //
        // Otherwise, we proceed with flushing like we normally would.
        let can_write_record = self.can_write_record(token.serialized_len());
        let writer = self
            .writer
            .as_mut()
            .expect("writer should exist after `ensure_ready_for_write`");

        let (bytes_written, flush_result) = if can_write_record {
            // We always return errors here because flushing the record won't return a recoverable
            // error like `DataFileFull`, as that gets checked during archiving. The guard fires
            // Errored automatically on `?` exit.
            let result = writer.flush_record(token).await?;
            // Record is durable on disk; disarm so finalizers resolve as Delivered.
            record_finalizers.disarm();
            result
        } else {
            // The record would not fit given the current size of the buffer, so we need to recover it from the
            // writer and hand it back. This looks a little weird because we want to surface deserialize/decoding
            // errors if we encounter them, but if we recover the record successfully, we're returning
            // `Ok(Err(record))` to signal that our attempt failed but the record is able to be retried again later.
            //
            // Reattach the finalizers: they were taken before archiving to handle the unwritable-
            // record path, but this record is being returned for retry (block mode) or overflow.
            // `into_inner` extracts from the guard without resolving status; the finalizers will
            // be resolved only when the returned record is eventually written or dropped.
            let mut record = writer.recover_archived_record(&token)?;
            record.merge_finalizer_groups(record_finalizers.into_inner());
            return Ok(Err(record));
        };

        // Track our write since things appear to have succeeded. This only updates our internal
        // state as we have not yet authoritatively flushed the write to the data file. This tracks
        // not only how many bytes we have buffered, but also how many events, which in turn drives
        // record ID generation.  We do this after the write appears to succeed to avoid issues with
        // setting the ledger state to a record ID that we may never have actually written, which
        // could lead to record ID gaps.
        self.track_write(record_events.get(), bytes_written as u64);

        // If we did flush some buffered writes during this write, however, we now compensate for
        // that after updating our internal state. Publishing the flushed state also notifies the
        // reader, after all shared state reflects the readable bytes.
        if let Some(flush_result) = flush_result {
            self.publish_flushed_progress(flush_result.events_flushed, flush_result.bytes_flushed);
        }

        // A record at or above the write-buffer size forces the buffered writer to
        // flush mid-record, exercising the large-record path that splits a single
        // record across multiple underlying writes.
        #[cfg(feature = "antithesis-disk-asserts")]
        {
            #![allow(clippy::disallowed_types)] // once_cell::Lazy
            antithesis_sdk::assert_sometimes!(
                bytes_written >= self.config.write_buffer_size,
                "a record at or over the write-buffer size is written",
                &serde_json::json!({
                    "bytes_written": bytes_written,
                    "write_buffer_size": self.config.write_buffer_size,
                })
            );
        }

        trace!(
            record_id,
            record_events,
            bytes_written,
            data_file_id = self.ledger.get_current_writer_file_id(),
            "Wrote record."
        );

        Ok(Ok(bytes_written))
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
    #[instrument(skip_all, level = "debug")]
    pub async fn write_record(&mut self, mut record: T) -> Result<usize, WriterError<T>> {
        loop {
            match self.try_write_record_inner(record).await? {
                Ok(bytes_written) => return Ok(bytes_written),
                Err(old_record) => {
                    record = old_record;
                    self.ledger.wait_for_reader().await;
                }
            }
        }
    }

    /// Writes a record, preserving whether the blocking write completed by dropping an unwritable
    /// record.
    ///
    /// Unlike [`Self::try_write_record`], this waits for reader progress when the buffer is full.
    #[instrument(skip_all, level = "debug")]
    pub async fn write_record_outcome(
        &mut self,
        record: T,
    ) -> Result<TryWriteOutcome<T>, WriterError<T>> {
        // `write_record` reports a zero-byte write as the sentinel for an unwritable record that
        // was dropped; every other byte count means the record was written.
        match self.write_record(record).await? {
            0 => Ok(TryWriteOutcome::Dropped),
            _ => Ok(TryWriteOutcome::Written),
        }
    }

    #[instrument(skip(self), level = "debug")]
    async fn flush_inner(&mut self, force_full_flush: bool) -> io::Result<()> {
        // We always flush the `BufWriter` when this is called, but we don't always flush to disk or
        // flush the ledger.  This is enough for readers on Linux since the file ends up in the page
        // cache, as we don't do any O_DIRECT fanciness, and the new contents can be immediately
        // read.
        //
        // TODO: Windows has a page cache as well, and macOS _should_, but we should verify this
        // behavior works on those platforms as well.
        let flush_result = if let Some(writer) = self.writer.as_mut() {
            writer.flush().await?
        } else {
            None
        };

        if let Some(flush_result) = flush_result {
            // Publish the readable bytes before waking the reader.
            self.publish_flushed_progress(flush_result.events_flushed, flush_result.bytes_flushed);
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
        self.flush_inner(false).await?;
        Ok(())
    }
}

impl<T, FS> BufferWriter<T, FS>
where
    FS: Filesystem,
    FS::File: Unpin,
{
    /// Closes this [`Writer`], marking it as done.
    ///
    /// Closing the writer signals to the reader that no more records will be written until the
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

impl<T, FS> Drop for BufferWriter<T, FS>
where
    FS: Filesystem,
    FS::File: Unpin,
{
    fn drop(&mut self) {
        self.close();
    }
}
