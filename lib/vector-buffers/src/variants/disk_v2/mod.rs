//! # Disk buffer v2.
//!
//! This disk buffer implementation focuses on a simplistic on-disk format with minimal
//! reader/writer coordination, and no exotic I/O techniques, such that the buffer is easy to write
//! to and read from and can provide simplistic, but reliable, recovery mechanisms when errors or
//! corruption are encountered.
//!
//! ## Design constraints
//!
//! These constraints, or more often, invariants, are the groundwork for ensuring that the design
//! can stay simple and understandable:
//!
//! - data files do not exceed 128MB
//! - no more than 65,536 data files can exist at any given time
//! - buffer can grow to a maximum of ~8TB in total size (65k files * 128MB)
//! - all records are checksummed (CRC32C)
//! - all records are written sequentially/contiguously, and do not span over multiple data files
//! - writers create and write to data files, while readers read from and delete data files
//! - endianness of the files is based on the host system (we don't support loading the buffer files
//!   on a system with different endianness)
//!
//! ## High-level design
//!
//! ### Records
//!
//! A record is an length-prefixed payload, where an arbitrary number of bytes are contained,
//! alongside a monotonically increasing ID, and protected by a CRC32C checksum. Since a record
//! simply stores opaque bytes, one or more events can be stored per record.
//!
//! The writer assigns record IDs based on number of events written to a record, such that a record
//! ID of N can be determined to contain M-N events, where M is the record ID of the next record.
//!
//! #### On-disk format
//!
//! Records are represented by the following pseudo-structure:
//!
//! ```text
//! record:
//!   record_len: uint64
//!   checksum:   uint32(CRC32C of record_id + payload)
//!   record_id:  uint64
//!   payload:    uint8[record_len]
//! ```
//!
//! We say "pseudo-structure" as a helper serialization library, [`rkyv`][rkyv], is used to handle
//! serialization, and zero-copy deserialization, of records. This effectively adds some amount of
//! padding to record fields, due to the need to structure record field data in a way that makes it
//! transparent to access during zero-copy deserialization, when the raw buffer of a record that was
//! read is able to be accessed as if it was a native Rust type/value.
//!
//! While this padding/overhead is small, and fixed, we do not quantify it here as it can
//! potentially changed based on the payload that a record contains. The only safe way to access the
//! records in a disk buffer should be through the reader/writer interface in this module.
//!
//! ### Data files
//!
//! Data files contain the buffered records and nothing else. Records are written
//! sequentially/contiguously, and are not padded out to meet a minimum block/write size, except for
//! internal padding requirements of the serialization library used.
//!
//! Data files have a maximum size, configured statically within a given Vector binary, which can
//! never be exceeded: if a write would cause a data file to grow past the maximum file size, it
//! must be written to the next data file.
//!
//! A maximum number of 65,536 data files can exist at any given time, due to the inclusion of a
//! file ID in the data file name, which is represented by a 16-bit unsigned integer.
//!
//! ### Ledger
//!
//! The ledger is a small file which tracks two important items for both the reader and writer:
//! which data file they're currently reading or writing to, and what record ID they left off on.
//!
//! The ledger is read during buffer initialization to determine a reader should pick up reading
//! from, but is also used to attempt to detect where a writer left off, and if records are missing
//! from the current writer data file according to what the writer believes it did (as in
//! write/flush bytes to disk) and what the reality is, based on the actual data in the current
//! writer data file.
//!
//! The ledger is a memory-mapped file that is updated atomically in terms of its fields, but is not
//! updated atomically in terms of reader/writer activity.
//!
//! #### On-disk format
//!
//! Like records, the ledger file consists of a simplified structure that is optimized for being shared
//! via a memory-mapped file interface between the reader and writer.
//!
//! ```text
//! buffer.db:
//!   writer_next_record_id:       uint64
//!   writer_current_data_file_id: uint16
//!   reader_current_data_file_id: uint16
//!   reader_last_record_id:       uint64
//! ```
//!
//! As the disk buffer structure is meant to emulate a ring buffer, most of the bookkeeping resolves
//! around the writer and reader being able to quickly figure out where they left off. Record and
//! data file IDs are simply rolled over when they reach the maximum of their data type, and are
//! incremented monotonically as new data files are created, rather than trying to always allocate
//! from the lowest available ID.
//!
//! ## Buffer operation
//!
//! ### Writing records
//!
//! As mentioned above, records are added to a data file sequentially, and contiguously, with no
//! gaps or data alignment adjustments, excluding the padding/alignment used by `rkyv` itself to
//! allow for zero-copy deserialization. This continues until adding another would exceed the
//! configured data file size limit. When this occurs, the current data file is flushed and
//! synchronized to disk, and a new data file will be opened.
//!
//! If the number of data files on disk exceeds the maximum (65,536), or if the total data file size
//! limit is exceeded, the writer will wait until enough space has been freed such that the record
//! can be written. As data files are only deleted after being read entirely, this means that space
//! is recovered in increments of the target data file size, which is 128MB. Thus, the minimum size
//! for a buffer must be equal to or greater than the target size of a single data file.
//! Additionally, as data files are uniquely named based on an incrementing integer, of which will
//! wrap around at 65,536 (2^16), the maximum data file size in total for a given buffer is ~8TB (6
//! 5k files * 128MB).
//!
//! ### Reading records
//!
//! Due to the on-disk layout, reading records is an incredibly straight-forward progress: we open a
//! file, read it until there's no more data and we know the writer is done writing to the file, and
//! then we open the next one, and repeat the process.
//!
//! ### Deleting acknowledged records
//!
//! As the reader emits records, we cannot yet consider them fully processed until they are
//! acknowledged. The acknowledgement process is tied into the normal acknowledgement machinery, and
//! the reader collects and processes those acknowledgements incrementally as reads occur.
//!
//! When all records from a data file have been fully acknowledged, the data file is scheduled for
//! deletion. We only delete entire data files, rather than truncating them piecemeal, which reduces
//! the I/O burden of the buffer. This does mean, however, that a data file will stick around until
//! it is entirely processed and acknowledged. We compensate for this fact in the buffer
//! configuration by adjusting the logical buffer size based on when records are acknowledged, so
//! that the writer can make progress as records are acknowledged, even if the buffer is close to,
//! or at the maximum buffer size limit.
//!
//! ### Record ID generation, and its relation of events
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
//! [rkyv]: https://docs.rs/rkyv

use core::fmt;
use std::{
    error::Error,
    marker::PhantomData,
    num::NonZeroU64,
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use snafu::{ResultExt, Snafu};
use vector_common::finalization::Finalizable;

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

use self::ledger::Ledger;
pub use self::{
    common::{DiskBufferConfig, DiskBufferConfigBuilder},
    io::{Filesystem, ProductionFilesystem},
    ledger::LedgerLoadCreateError,
    reader::{BufferReader, ReaderError},
    writer::{BufferWriter, WriterError},
};
use crate::{
    buffer_usage_data::BufferUsageHandle,
    topology::{
        builder::IntoBuffer,
        channel::{ReceiverAdapter, SenderAdapter},
    },
    Bufferable,
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
    ) -> Result<(BufferWriter<T, FS>, BufferReader<T, FS>, Arc<Ledger<FS>>), BufferError<T>>
    where
        FS: Filesystem + fmt::Debug + Clone + 'static,
        FS::File: Unpin,
    {
        let ledger = Ledger::load_or_create(config, usage_handle)
            .await
            .context(LedgerSnafu)?;
        let ledger = Arc::new(ledger);

        let mut writer = BufferWriter::new(Arc::clone(&ledger));
        writer
            .validate_last_write()
            .await
            .context(WriterSeekFailedSnafu)?;

        let finalizer = Arc::clone(&ledger).spawn_finalizer();

        let mut reader = BufferReader::new(Arc::clone(&ledger), finalizer);
        reader
            .seek_to_next_record()
            .await
            .context(ReaderSeekFailedSnafu)?;

        ledger.synchronize_buffer_usage();

        Ok((writer, reader, ledger))
    }

    /// Creates a new disk buffer from the given [`DiskBufferConfig`].
    ///
    /// If successful, a [`Writer`] and [`Reader`] value, representing the write/read sides of the
    /// buffer, respectively, will be returned. Records are considered durably processed and able
    /// to be deleted from the buffer when they are dropped by the reader, via event finalization.
    ///
    /// # Errors
    ///
    /// If an error occurred during the creation or loading of the disk buffer, an error variant
    /// will be returned describing the error.
    #[cfg_attr(test, instrument(skip(config, usage_handle), level = "trace"))]
    pub async fn from_config<FS>(
        config: DiskBufferConfig<FS>,
        usage_handle: BufferUsageHandle,
    ) -> Result<(BufferWriter<T, FS>, BufferReader<T, FS>), BufferError<T>>
    where
        FS: Filesystem + fmt::Debug + Clone + 'static,
        FS::File: Unpin,
    {
        let (writer, reader, _) = Self::from_config_inner(config, usage_handle).await?;

        Ok((writer, reader))
    }
}

pub struct DiskV2Buffer {
    id: String,
    data_dir: PathBuf,
    max_size: NonZeroU64,
}

impl DiskV2Buffer {
    pub fn new(id: String, data_dir: PathBuf, max_size: NonZeroU64) -> Self {
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
    T: Bufferable + Clone + Finalizable,
{
    fn provides_instrumentation(&self) -> bool {
        true
    }

    async fn into_buffer_parts(
        self: Box<Self>,
        usage_handle: BufferUsageHandle,
    ) -> Result<(SenderAdapter<T>, ReceiverAdapter<T>), Box<dyn Error + Send + Sync>> {
        let (writer, reader) = build_disk_v2_buffer(
            usage_handle,
            &self.data_dir,
            self.id.as_str(),
            self.max_size,
        )
        .await?;

        Ok((writer.into(), reader.into()))
    }
}

async fn build_disk_v2_buffer<T>(
    usage_handle: BufferUsageHandle,
    data_dir: &Path,
    id: &str,
    max_size: NonZeroU64,
) -> Result<
    (
        BufferWriter<T, ProductionFilesystem>,
        BufferReader<T, ProductionFilesystem>,
    ),
    Box<dyn Error + Send + Sync>,
>
where
    T: Bufferable + Clone,
{
    usage_handle.set_buffer_limits(Some(max_size.get()), None);

    let buffer_path = get_disk_v2_data_dir_path(data_dir, id);
    let config = DiskBufferConfigBuilder::from_path(buffer_path)
        .max_buffer_size(max_size.get())
        .build()?;
    Buffer::from_config(config, usage_handle)
        .await
        .map_err(Into::into)
}

pub(crate) fn get_disk_v2_data_dir_path(base_dir: &Path, buffer_id: &str) -> PathBuf {
    base_dir.join("buffer").join("v2").join(buffer_id)
}
