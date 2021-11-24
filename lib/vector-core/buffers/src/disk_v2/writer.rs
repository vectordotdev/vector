use std::{
    io::{self, ErrorKind},
    marker::PhantomData,
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

use crate::{
    bytes::EncodeBytes,
    disk_v2::{
        ledger::Ledger,
        record::{Record, RecordStatus},
    },
    Bufferable,
};

use super::{backed_archive::BackedArchive, common::BufferConfig};

#[derive(Debug, Snafu)]
pub enum WriterError<R>
where
    R: Bufferable,
    <R as EncodeBytes<R>>::Error: 'static,
{
    #[snafu(display("write I/O error: {}", source))]
    Io { source: io::Error },
    #[snafu(display("record too large: limit is {}", limit))]
    RecordTooLarge { limit: usize },
    #[snafu(display("failed to encode record: {:?}", source))]
    FailedToEncode {
        source: <R as EncodeBytes<R>>::Error,
    },
    #[snafu(display("failed to serialize encoded record to buffer: {}", reason))]
    FailedToSerialize { reason: String },
}

pub struct RecordWriter<W, T> {
    writer: BufWriter<W>,
    encode_buf: Vec<u8>,
    ser_buf: AlignedVec,
    ser_scratch: AlignedVec,
    checksummer: Hasher,
    max_record_size: usize,
    _t: PhantomData<T>,
}

impl<W, T> RecordWriter<W, T>
where
    W: AsyncWrite + Unpin,
    T: Bufferable,
{
    /// Creates a new `RecordWriter` around the provided writer.
    ///
    /// Internally, the writer is wrapped in a `BufWriter<W>`, so callers should not pass in an
    /// already buffered writer.
    pub fn new(writer: W, record_max_size: usize) -> Self {
        Self {
            writer: BufWriter::new(writer),
            encode_buf: Vec::with_capacity(16_384),
            ser_buf: AlignedVec::with_capacity(16_384),
            ser_scratch: AlignedVec::with_capacity(16_384),
            checksummer: Hasher::new(),
            max_record_size: record_max_size,
            _t: PhantomData,
        }
    }

    /// Gets a reference to the underlying writer.
    pub fn get_ref(&self) -> &W {
        self.writer.get_ref()
    }

    /// Writes a record.
    ///
    /// Returns the number of bytes written to serialize the record, including the framing. Writes
    /// are not automatically flushed, so `flush` must be called after any record write if there is
    /// a requirement for the record to immediately be written all the way to the underlying writer.
    ///
    /// # Errors
    ///
    /// If there is an I/O error while writing the record, an error variant will be returned
    /// describing the error.  Additionally, if there is an error while serializing the record, an
    /// error variant will be returned describing the serialization error.
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
        // record size, when encoded, exceeds our limit.
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
            .context(FailedToEncode)?;
        if encoded_len >= self.max_record_size {
            return Err(WriterError::RecordTooLarge {
                limit: self.max_record_size,
            });
        }

        let record = Record::with_checksum(id, &self.encode_buf, &self.checksummer);

        // Now serialize the record, which puts it into its archived form.  This is what powers our
        // ability to do zero-copy deserialization from disk.
        //
        // NOTE: This operation is put into its own block scope because otherwise `serializer` lives
        // untilk the end of the function, and it contains a mutable buffer pointer, which is
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

            match serializer.serialize_value(&record) {
                Ok(_) => Ok(serializer.pos()),
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
        assert_eq!(self.ser_buf.len(), archive_len as usize);

        let wire_archive_len: u32 = archive_len
            .try_into()
            .expect("archive len should always fit into a u32");
        self.writer
            .write_all(&wire_archive_len.to_be_bytes()[..])
            .await
            .context(Io)?;
        self.writer.write_all(&self.ser_buf).await.context(Io)?;

        Ok(4 + archive_len)
    }

    /// Flushes the writer.
    ///
    /// This flushes both the internal buffered writer and the underlying writer object.
    ///
    /// # Errors
    ///
    /// If there is an I/O error while flushing either the buffered writer or the underlying writer,
    /// an error variant will be returned describing the error.
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
    pub async fn sync_all(&mut self) -> io::Result<()> {
        self.writer.get_mut().sync_all().await
    }
}

pub struct Writer<T> {
    ledger: Arc<Ledger>,
    config: BufferConfig,
    writer: Option<RecordWriter<File, T>>,
    data_file_size: u64,
    _t: PhantomData<T>,
}

impl<T> Writer<T>
where
    T: Bufferable,
{
    pub(crate) fn new(ledger: Arc<Ledger>) -> Self {
        let config = ledger.config().clone();
        Writer {
            ledger,
            config,
            writer: None,
            data_file_size: 0,
            _t: PhantomData,
        }
    }

    fn track_write(&mut self, record_size: u64) {
        self.data_file_size += record_size;
        self.ledger.track_write(record_size);
    }

    fn can_write(&mut self) -> bool {
        self.data_file_size < self.config.max_data_file_size
    }

    fn reset(&mut self) {
        self.writer = None;
        self.data_file_size = 0;
    }

    /// Validates that the last write in the current writer data file matches the ledger.
    pub async fn validate_last_write(&mut self) -> Result<(), WriterError<T>> {
        self.ensure_ready_for_write().await.context(Io)?;

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
            .expect("writer should exist after ensure_ready_for_write")
            .get_ref()
            .try_clone()
            .await
            .context(Io)?
            .into_std()
            .await;

        let data_file_mmap = unsafe { Mmap::map(&data_file_handle).context(Io)? };

        // We have bytes, so we should have an archived record.  Mind you, this could be a partial
        // write if there was an error so we still need to use `BackedArchive` which will check for
        // us and ensure we don't try to erroneously map partial bytes to `archivedRecord`.
        match BackedArchive::<_, Record<'_>>::from_backing(data_file_mmap) {
            // We got a record (which may or may not be corrupted, still gotta check!) so keep going.
            Ok(archive) => {
                let record = archive.get_archive_ref();
                match record.verify_checksum(&Hasher::new()) {
                    RecordStatus::Valid(id) => {
                        // Since we have a valid record, checksum and all, see if the writer record ID
                        // in the ledger lines up with the record ID we have here.
                        let ledger_next_writer_record_id =
                            self.ledger.state().get_next_writer_record_id();
                        let last_written_record_id = id;

                        // TODO: do we actually want this? the existence of a difference greater
                        // than one implies there were records written that we tracked via the
                        // ledger, but that never made it to disk.  if we fix up the ledger "next
                        // writer record ID" file, we just hide that there was a problem, because
                        // otherwise the reader would eventually hit this and then the record ID gap
                        // logic would kick in to emit an event to show that we skipped/dropped a
                        // bunch of events..
                        //
                        // the best we could reasonably do here, I think, is to skip to the next
                        // data file to limit how many records we lose when we know the data file is
                        // in a bad state, even if it doesn't account for corrupted records in the
                        // middle of the file vs the last one.  we'd have to do that in the
                        // non-success arms of this match, though.
                        if ledger_next_writer_record_id - 1 != last_written_record_id {
                            println!("writer record ID mismatch: next ID (ledger) = {}, last ID (written) = {}",
                                ledger_next_writer_record_id, last_written_record_id);
                            println!("these values should always be one apart from each other");
                        }
                        Ok(())
                    }
                    RecordStatus::Corrupted { .. } => {
                        println!("got invalid checksum from record at end of current writer data file, boo!");
                        Ok(())
                    }
                    // `ArchivedRecord::verify_checksum` doesn't actually return the failed
                    // deserialization variant of `RecordStatus`, so we might want to switch to
                    // using `try_as_record_archive` here instead of using `BackedArchive`.
                    _ => unreachable!(),
                }
            }
            // Oh no, an error! There's nothing for us to do, really, since tghe reader has the
            // logic for skipping records and deleting files when corruption is detected, so just
            // let that happened, but spit out the error here for posterity.
            Err(e) => {
                println!(
                    "got error deserializing last record in the current writer data file: {}",
                    e.into_inner()
                );
                Ok(())
            }
        }
    }

    /// Ensures this writer is ready to attempt writer the next record.
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
            self.flush().await?;

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
                            .read(true)
                            .create(true)
                            .open(&data_file_path)
                            .await?;
                        let metadata = data_file.metadata().await?;
                        let file_len = metadata.len();
                        if file_len >= self.config.max_data_file_size {
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
                    data_file.sync_all().await?;

                    self.writer = Some(RecordWriter::new(data_file, self.config.max_record_size));
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

    /// Writes a record.
    pub async fn write_record(&mut self, record: T) -> Result<usize, WriterError<T>> {
        self.ensure_ready_for_write().await.context(Io)?;

        // Grab the next record ID and attempt to write the record.
        let id = self.ledger.state().get_next_writer_record_id();
        let n = self
            .writer
            .as_mut()
            .expect("writer should exist after ensure_ready_for_write")
            .write_record(id, record)
            .await?;

        // Since we succeeded in writing the record, increment the next record ID and metadata for
        // the writer.  We do this here to avoid consuming record IDs even if a write failed, as we
        // depend on the "record IDs are monotonic" invariant for detecting skipped records during read.
        self.ledger.state().increment_next_writer_record_id();
        self.track_write(n as u64);

        Ok(n)
    }

    /// Flushes the writer.
    ///
    /// This must be called for the reader to be able to make progress.
    ///
    /// This does not ensure that the data is fully synchronized (i.e. `fsync`) to disk, however it
    /// may sometimes perform a full synchronization if the time since the last full synchronization
    /// occurred has exceeded a configured limit.
    pub async fn flush(&mut self) -> io::Result<()> {
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

        if self.ledger.should_flush() {
            if let Some(writer) = self.writer.as_mut() {
                writer.sync_all().await?;
            }

            self.ledger.flush()
        } else {
            Ok(())
        }
    }

    pub fn get_ledger_state(&self) -> String {
        format!("{:#?}", self.ledger.state())
    }
}
