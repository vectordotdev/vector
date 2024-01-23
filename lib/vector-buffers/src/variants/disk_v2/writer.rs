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
    ser::{
        serializers::{
            AlignedSerializer, AllocScratch, AllocScratchError, BufferScratch, CompositeSerializer,
            CompositeSerializerError, FallbackScratch,
        },
        Serializer,
    },
    AlignedVec, Infallible,
};
use snafu::{ResultExt, Snafu};
use tokio::io::{AsyncWrite, AsyncWriteExt};

use super::{
    common::{create_crc32c_hasher, DiskBufferConfig},
    io::Filesystem,
    ledger::Ledger,
    record::{validate_record_archive, Record, RecordStatus},
};
use crate::{
    encoding::{AsMetadata, Encodable},
    variants::disk_v2::{
        io::AsyncFile,
        reader::decode_record_payload,
        record::{try_as_record_archive, RECORD_HEADER_LEN},
    },
    Bufferable,
};

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
    #[snafu(display("record too large: limit is {}", limit))]
    RecordTooLarge { limit: usize },

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

impl<T: Bufferable + PartialEq> PartialEq for WriterError<T> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Io { source: l_source }, Self::Io { source: r_source }) => {
                l_source.kind() == r_source.kind()
            }
            (Self::RecordTooLarge { limit: l_limit }, Self::RecordTooLarge { limit: r_limit }) => {
                l_limit == r_limit
            }
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
            return Err(WriterError::FailedToSerialize {
                reason: format!(
                    "serializer position invalid for context: pos={} len={}",
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
    pub fn recover_archived_record(&mut self, token: WriteToken) -> Result<T, WriterError<T>> {
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
        self.next_record_id.wrapping_add(self.unflushed_events)
    }

    fn track_write(&mut self, event_count: usize, record_size: u64) {
        self.data_file_size += record_size;
        self.unflushed_events += event_count as u64;
        self.unflushed_bytes += record_size;
    }

    fn flush_write_state(&mut self) {
        self.flush_write_state_partial(self.unflushed_events, self.unflushed_bytes);
    }

    fn flush_write_state_partial(&mut self, flushed_events: u64, flushed_bytes: u64) {
        debug_assert!(
            flushed_events <= self.unflushed_events,
            "tried to flush more events than are currently unflushed"
        );
        debug_assert!(
            flushed_bytes <= self.unflushed_bytes,
            "tried to flush more bytes than are currently unflushed"
        );

        self.next_record_id = self
            .ledger
            .state()
            .increment_next_writer_record_id(flushed_events);
        self.unflushed_events -= flushed_events;
        self.unflushed_bytes -= flushed_bytes;

        self.ledger.track_write(flushed_events, flushed_bytes);
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
                        let next_record_id = self
                            .ledger
                            .state()
                            .increment_next_writer_record_id(ledger_record_delta);
                        self.next_record_id = next_record_id;
                        self.unflushed_events = 0;

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
        // Essentially, we defer the actual skipping to avoid deadlocking here trying to open a
        // data file we might not be able to open yet.
        if should_skip_to_next_file {
            self.reset();
            self.mark_for_skip();
        }

        self.ready_to_write = true;

        Ok(())
    }

    fn is_buffer_full(&self) -> bool {
        let total_buffer_size = self.ledger.get_total_buffer_size() + self.unflushed_bytes;
        let max_buffer_size = self.config.max_buffer_size;
        total_buffer_size >= max_buffer_size
    }

    /// Ensures this writer is ready to attempt writer the next record.
    #[instrument(skip(self), level = "debug")]
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
            self.flush_write_state();

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

    /// Attempts to write a record.
    ///
    /// If the buffer is currently full, the original record will be immediately returned.
    /// Otherwise, a write will be executed, which will run to completion, and `None` will be returned.
    ///
    /// # Errors
    ///
    /// If an error occurred while writing the record, an error variant will be returned describing
    /// the error.
    pub async fn try_write_record(&mut self, record: T) -> Result<Option<T>, WriterError<T>> {
        self.try_write_record_inner(record)
            .await
            .map(|result| match result {
                Ok(_) => None,
                Err(record) => Some(record),
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

                        continue;
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
            // We always return errors here because flushing the record won't return a recoverable error like
            // `DataFileFull`, as that gets checked during archiving.
            writer.flush_record(token).await?
        } else {
            // The record would not fit given the current size of the buffer, so we need to recover it from the
            // writer and hand it back. This looks a little weird because we want to surface deserialize/decoding
            // errors if we encounter them, but if we recover the record successfully, we're returning
            // `Ok(Err(record))` to signal that our attempt failed but the record is able to be retried again later.
            return Ok(Err(writer.recover_archived_record(token)?));
        };

        // Track our write since things appear to have succeeded. This only updates our internal
        // state as we have not yet authoritatively flushed the write to the data file. This tracks
        // not only how many bytes we have buffered, but also how many events, which in turn drives
        // record ID generation.  We do this after the write appears to succeed to avoid issues with
        // setting the ledger state to a record ID that we may never have actually written, which
        // could lead to record ID gaps.
        self.track_write(record_events.get(), bytes_written as u64);

        // If we did flush some buffered writes during this write, however, we now compensate for
        // that after updating our internal state.  We'll also notify the reader, too, since the
        // data should be available to read:
        if let Some(flush_result) = flush_result {
            self.flush_write_state_partial(flush_result.events_flushed, flush_result.bytes_flushed);
            self.ledger.notify_writer_waiters();
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
                    continue;
                }
            }
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
        self.flush_inner(false).await?;
        self.flush_write_state();
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
