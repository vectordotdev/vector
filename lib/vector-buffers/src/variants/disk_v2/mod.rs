//! # Disk Buffer v2: Sequential File I/O Boogaloo.
//!
//! This disk buffer implementation is a departure from the LevelDB-based disk buffer
//! implementation, referred to internally as `disk` or `disk_v1`. It focuses on avoiding external
//! C/C++ dependencies, as well as optimizing itself for the job at hand to provide more consistent
//! in both throughput and latency.
//!
//! ## Design constraints
//!
//! These constraints, or more often, invariants, are the groundwork for ensuring that the design
//! can stay simple and understandable:
//! - data files do not exceed 128MB
//! - no more than 65,536 data files can exist at any given time
//! - buffer can grow to a maximum of ~8TB in total size (65k files * 128MB)
//! - all records are checksummed (CRC32C)
//! - all records are written sequentially/contiguously, and do not span over multiple data files
//! - writers create and write to data files, while readers read from and delete data files
//! - endianness of the files is based on the host system (we don't support loading the buffer files
//!   on a system with different endianness)
//!
//! ## On-disk layout
//!
//! At a high-level, records that are written end up in one of many underlying data files, while the
//! ledger file -- number of records, writer and reader positions, etc -- is stored in a separate
//! file. Data files function primarily with a "last process who touched it" ownership model: the
//! writer always creates new files, and the reader deletes files when they have been fully read.
//!
//! ### Record structure
//!
//! Records are packed together with a relatively simple pseudo-structure:
//!
//!   record:
//!     `record_len`: uint64
//!     `checksum`:   uint32(CRC32C of `record_id` + `payload`)
//!     `record_id`:  uint64
//!     `payload`:    uint8[]
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
//! ## Writing records
//!
//! Records are added to a data file sequentially, and contiguously, with no gaps or data alignment
//! adjustments, excluding the padding/alignment used by `rkyv` itself to allow for zero-copy
//! deserialization. This continues until adding another would exceed the configured data file size
//! limit. When this occurs, the current data file is flushed and synchronized to disk, and a new
//! data file will be open.
//!
//! If the number of data files open exceeds the maximum (65,536), or if the total data file size
//! limit is exceeded, the writer will wait until enough space has been freed such that the record
//! can be written. As data files are only deleted after being read entirely, this means that space
//! is recovered in increments of the target data file size, which is 128MB. Thus, the minimum size
//! for a buffer must be equal to or greater than the target size of a single data file.
//! Additionally, as data files are uniquely named based on an incrementing integer, of which will
//! wrap around at 65,536 (2^16), the maximum data file size in total for a given buffer is ~8TB (6
//! 5k files * 128MB).
//!
//! ## Reading records
//!
//! Due to the on-disk layout, reading records is an incredibly straight-forward progress: we open a
//! file, read it until there's no more data and we know the writer is done writing to the file, and
//! then we open the next one, and repeat the process.
//!
//! ## Deleting acknowledged records
//!
//! As the reader emits records, we cannot yet consider them fully processed until they are
//! acknowledged. The acknowledgement process is tied into the normal acknowledgement machinery, and
//! the reader collects and processes those acknowledgements incrementally as reads occur.
//!
//! When all records from a data file have been fully acknowledged, the data file is scheduled for
//! deletion. We only delete entire data files, rather than truncating them piecemeal, which reduces
//! the I/O burden of the buffer. This does mean, however, that a data file will stick around until
//! it's entirely processed. We compensate for this fact in the buffer configuration by adjusting
//! the logical buffer size based on when records are acknowledged, so that the writer can make
//! progress as records are acknowledged, even if the buffer is close to, or at the maximum buffer
//! size limit.
//!
//! ## Record ID generation, and its relation of events
//!
//! While the buffer talks a lot about writing "records", records are ostensibly a single event, or
//! collection of events. We manage the organization and grouping of events at at a higher level
//! (i.e. `EventArray`), but we're still required to confront this fact at the buffer layer. In
//! order to maintain as little extra metadata as possible as records, and within the ledger, we
//! encode the number of events in a buffer into the record ID. We do this by using the value
//! returned by `EventCount::event_count` on a per-record basis.
//!
//! For example, a fresh buffer starts at a record ID of 1 for the writer: that is, the next write
//! will start at 1. If we write a record that contains 10 events, we add that event count to the
//! record ID we started from, which gives us 11. The next record write will start at 11, and the
//! pattern continues going forward.
//!
//! The other reason we do this is to allow us to quickly and easily determine how many events exist
//! in a buffer. Since we have the invariant of knowing that record IDs are tied, in a way, to event
//! count, we can quickly and easily find the first and last unread record in the buffer, and do
//! simple subtraction to calculate how many events we have outstanding. While there is logic that
//! handles corrupted records, or other corner case errors, the core premise, and logic, follows
//! this pattern.
//!
//! We need to track our reader progress, both in the form of how much data we've read in this data
//! file, as well as the record ID. This is required not only for ensuring our general buffer
//! accounting (event count, buffer size, etc) is accurate, but also to be able to handle corrupted
//! records.
//!
//! We make sure to track enough information such that when we encounter a corrupted record, or if
//! we skip records due to missing data, we can figure out how many events we've dropped or lost,
//! and handle the necessary adjustments to the buffer accounting.
//!
//! ## Ledger structure
//!
//! Likewise, the ledger file consists of a simplified structure that is optimized for being shared
//! via a memory-mapped file interface between the writer and reader. Like the record structure, the
//! below is a pseudo-structure as we use `rkyv` for the ledger, and so the on-disk layout will be
//! slightly different:
//!
//!   buffer.db:
//!     writer next record ID:       uint64
//!     writer current data file ID: uint16
//!     reader current data file ID: uint16
//!     reader last record ID:       uint64
//!
//! As the disk buffer structure is meant to emulate a ring buffer, most of the bookkeeping resolves
//! around the writer and reader being able to quickly figure out where they left off. Record and
//! data file IDs are simply rolled over when they reach the maximum of their data type, and are
//! incremented monotonically as new data files are created, rather than trying to always allocate
//! from the lowest available ID.
//!
//! Additionally, record IDs are allocated in the same way: monotonic, sequential, and will wrap
//! when they reach the maximum value for the data type. For record IDs, however, this would mean
//! reaching 2^64, which will take a really, really, really long time.

use std::{
    error::Error,
    marker::PhantomData,
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use async_trait::async_trait;
use futures::{ready, SinkExt, Stream};
use pin_project::pin_project;
use snafu::{ResultExt, Snafu};
use tokio::sync::mpsc::{channel, Receiver};
use tokio_util::sync::{PollSender, ReusableBoxFuture};

mod acknowledgements;
mod backed_archive;
mod common;
mod io;
mod ledger;
mod reader;
mod record;
mod ser;
mod writer;

#[cfg(test)]
mod tests;

use self::{acknowledgements::create_disk_v2_acker, ledger::Ledger};
pub use self::{
    common::{DiskBufferConfig, DiskBufferConfigBuilder},
    io::Filesystem,
    ledger::LedgerLoadCreateError,
    reader::{Reader, ReaderError},
    writer::{Writer, WriterError},
};
use crate::{
    buffer_usage_data::BufferUsageHandle,
    topology::{
        builder::IntoBuffer,
        channel::{ReceiverAdapter, SenderAdapter},
    },
    Acker, Bufferable,
};

/// Error that occurred when creating/loading a disk buffer.
#[derive(Debug, Snafu)]
pub enum BufferError<T>
where
    T: Bufferable,
{
    /// Failed to create/load the ledger.
    #[snafu(display("failed to load/create ledger: {}", source))]
    LedgerError { source: LedgerLoadCreateError },

    /// Failed to initialize/catch the reader up to where it left off.
    #[snafu(display("failed to seek to position where reader left off: {}", source))]
    ReaderSeekFailed { source: ReaderError<T> },

    /// Failed to initialize/catch the writer up to where it left off.
    #[snafu(display("failed to seek to position where writer left off: {}", source))]
    WriterSeekFailed { source: WriterError<T> },
}

/// Helper type for creating a disk buffer.
pub struct Buffer<T> {
    _t: PhantomData<T>,
}

impl<T> Buffer<T>
where
    T: Bufferable,
{
    #[cfg_attr(test, instrument(skip(config, usage_handle), level = "trace"))]
    pub(crate) async fn from_config_inner<FS>(
        config: DiskBufferConfig<FS>,
        usage_handle: BufferUsageHandle,
    ) -> Result<(Writer<T, FS>, Reader<T, FS>, Acker, Arc<Ledger<FS>>), BufferError<T>>
    where
        FS: Filesystem + Clone + 'static,
        FS::File: Unpin,
    {
        let ledger = Ledger::load_or_create(config, usage_handle)
            .await
            .context(LedgerSnafu)?;
        let ledger = Arc::new(ledger);

        let mut writer = Writer::new(Arc::clone(&ledger));
        writer
            .validate_last_write()
            .await
            .context(WriterSeekFailedSnafu)?;

        let mut reader = Reader::new(Arc::clone(&ledger));
        reader
            .seek_to_next_record()
            .await
            .context(ReaderSeekFailedSnafu)?;

        ledger.synchronize_buffer_usage();

        let acker = create_disk_v2_acker(Arc::clone(&ledger));

        Ok((writer, reader, acker, ledger))
    }

    /// Creates a new disk buffer from the given [`DiskBufferConfig`].
    ///
    /// If successful, a [`Writer`] and [`Reader`] value, representing the write/read sides of the
    /// buffer, respectively, will be returned. Additionally, an [`Acker`] will be returned, which
    /// must be used to indicate when records read from the [`Reader`] can be considered durably
    /// processed and able to be deleted from the buffer.
    ///
    /// # Errors
    ///
    /// If an error occurred during the creation or loading of the disk buffer, an error variant
    /// will be returned describing the error.
    #[cfg_attr(test, instrument(skip(config, usage_handle), level = "trace"))]
    pub async fn from_config<FS>(
        config: DiskBufferConfig<FS>,
        usage_handle: BufferUsageHandle,
    ) -> Result<(Writer<T, FS>, Reader<T, FS>, Acker), BufferError<T>>
    where
        FS: Filesystem + Clone + 'static,
        FS::File: Unpin,
    {
        let (writer, reader, acker, _) = Self::from_config_inner(config, usage_handle).await?;

        Ok((writer, reader, acker))
    }
}

pub struct DiskV2Buffer {
    id: String,
    data_dir: PathBuf,
    max_size: u64,
}

impl DiskV2Buffer {
    pub fn new(id: String, data_dir: PathBuf, max_size: u64) -> Self {
        Self {
            id,
            data_dir,
            max_size,
        }
    }
}

#[async_trait]
impl<T> IntoBuffer<T> for DiskV2Buffer
where
    T: Bufferable + Clone,
{
    fn provides_instrumentation(&self) -> bool {
        true
    }

    async fn into_buffer_parts(
        self: Box<Self>,
        usage_handle: BufferUsageHandle,
    ) -> Result<(SenderAdapter<T>, ReceiverAdapter<T>, Option<Acker>), Box<dyn Error + Send + Sync>>
    {
        usage_handle.set_buffer_limits(Some(self.max_size), None);

        // Create the actual buffer subcomponents.
        let buffer_path = self.data_dir.join("buffer").join("v2").join(self.id);
        let config = DiskBufferConfigBuilder::from_path(buffer_path)
            .max_buffer_size(self.max_size as u64)
            .build()?;
        let (writer, reader, acker) = Buffer::from_config(config, usage_handle).await?;

        let wrapped_reader = WrappedReader::new(reader);

        let (input_tx, input_rx) = channel(1024);
        tokio::spawn(drive_disk_v2_writer(writer, input_rx));

        Ok((
            SenderAdapter::opaque(PollSender::new(input_tx).sink_map_err(|_| ())),
            ReceiverAdapter::opaque(wrapped_reader),
            Some(acker),
        ))
    }
}

#[pin_project]
struct WrappedReader<T, FS>
where
    FS: Filesystem,
{
    #[pin]
    reader: Option<Reader<T, FS>>,
    read_future: ReusableBoxFuture<'static, (Reader<T, FS>, Option<T>)>,
}

impl<T, FS> WrappedReader<T, FS>
where
    T: Bufferable,
    FS: Filesystem + 'static,
    FS::File: Unpin,
{
    pub fn new(reader: Reader<T, FS>) -> Self {
        Self {
            reader: Some(reader),
            read_future: ReusableBoxFuture::new(make_read_future(None)),
        }
    }
}

impl<T, FS> Stream for WrappedReader<T, FS>
where
    T: Bufferable,
    FS: Filesystem + 'static,
    FS::File: Unpin,
{
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        loop {
            match this.reader.as_mut().get_mut().take() {
                None => {
                    let (reader, result) = ready!(this.read_future.poll(cx));
                    this.reader.set(Some(reader));
                    return Poll::Ready(result);
                }
                Some(reader) => this.read_future.set(make_read_future(Some(reader))),
            }
        }
    }
}

async fn make_read_future<T, FS>(reader: Option<Reader<T, FS>>) -> (Reader<T, FS>, Option<T>)
where
    T: Bufferable,
    FS: Filesystem,
    FS::File: Unpin,
{
    match reader {
        None => unreachable!("future should not be called in this state"),
        Some(mut reader) => {
            let result = match reader.next().await {
                Ok(result) => result,
                Err(e) => {
                    // TODO: We can _probably_ avoid having to actually kill the task here,
                    // because the reader will recover from read errors, but, things it won't
                    // automagically recover from:
                    // - if it rolls to the next data file mid-data file, the writer might still be
                    //   writing more records to the current data file, which means we might stall
                    //   reads until the writer needs to roll to the next data file:
                    //
                    //   maybe there's an easy way we could propagate the rollover events to the
                    //   writer to also get it to rollover?  again, more of a technique to minimize
                    //   the number of records we throw away by rolling over. this could be tricky
                    //   to accomplish, though, for in-flight readers, but it's just a thought in a
                    //   code comment for now.
                    //
                    // - actual I/O errors like a failed read or permissions or whatever:
                    //
                    //   we haven't fully quantified what it means for the reader to get an
                    //   I/O error during a read, since we could end up in an inconsistent state if
                    //   the I/O error came mid-record read, after already reading some amount of
                    //   data and then losing our place by having the "wait for the data" code break
                    //   out with the I/O error.
                    //
                    //   this could be a potential enhancement to the reader where we also use the
                    //   "bytes read" value as the position in the data file, and track error state
                    //   internally, such that any read that was interrupted by a true I/O error
                    //   will set the error state and inform the next call to `try_read_record` to
                    //   seek back to the position prior to the read and to clear the read buffers,
                    //   enabling a clean-slate attempt.
                    //
                    //   regardless, such an approach might only be acheivable for specific I/O
                    //   errors and we could _potentially_ end up spamming the logs i.e. if a file
                    //   has its permissions modified and it just keeps absolutely blasting the logs
                    //   with the above error that we got from the reader.. maybe it's better to
                    //   spam the logs to indicate an error if it's possible to fix it? the reader
                    //   _could_ pick back up if permissions were fixed, etc...
                    error!("error during disk buffer read: {}", e);
                    None
                }
            };

            (reader, result)
        }
    }
}

async fn drive_disk_v2_writer<T, FS>(mut writer: Writer<T, FS>, mut input: Receiver<T>)
where
    T: Bufferable,
    FS: Filesystem + Clone,
    FS::File: Unpin,
{
    // TODO: Use a control message approach so callers can send both items to write and flush
    // requests, facilitating the ability to allow for `send_all` at the frontend.
    while let Some(record) = input.recv().await {
        if let Err(e) = writer.write_record(record).await {
            error!("failed to write record to the buffer: {}", e);
        }

        if let Err(e) = writer.flush().await {
            error!("failed to flush the buffer: {}", e);
        }
    }

    trace!("diskv2 writer task finished");
}
