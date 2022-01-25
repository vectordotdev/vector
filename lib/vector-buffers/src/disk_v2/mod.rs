//! # Disk Buffer v2: Sequential File I/O Boogaloo.
//!
//! This disk buffer implementation is a replace from the LevelDB-based disk buffer implementation,
//! referred internal to `disk` or `disk_v1`.  It focuses on avoiding external C/C++ dependencies,
//! as well as optimizing itself for the job at hand to provide more consistent in both throughput
//! and latency.
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
//! file.  Data files function primarily with a "last process who touched it" ownership model: the
//! writer always creates new files, and the reader deletes files when they have been fully read.
//!
//! ### Record structure
//!
//! Records are packed together with a relatively simple pseudo-structure:
//!   record:
//!     `record_len`: uint64
//!     `checksum`: uint32 (CRC32C of `record_id` + `payload`)
//!     `record_id`: uint64
//!     `payload`: uint8[]
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
//! ### Writing records
//!
//! Records are added to a data file sequentially, and contiguously, with no gaps or data alignment
//! adjustments, excluding the padding/alignment used by `rkyv` itself to allow for zero-copy
//! deserialization. This continues until adding another would exceed the configured data file size
//! limit.  When this occurs, the current data file is flushed and synchronized to disk, and a new
//! data file will be open.
//!
//! If the number of data files open exceeds the maximum (65,536), or if the total data file size
//! limit is exceeded, the writer will wait until enough space has been freed such that the record
//! can be written.  As data files are only deleted after being read entirely, this means that space
//! is recovered in increments of the target data file size, which is 128MB.  Thus, the minimum size
//! for a buffer must be equal to or greater than the target size of a single data file.
//! Additionally, as data files are uniquely named based on an incrementing integer, of which will
//! wrap around at 65,536 (2^16), the maximum data file size in total for a given buffer is ~8TB (6
//! 5k files * 128MB).
//!
//! Additionally, if the configured maximum data size would be exceeded, then a writer will wait for
//! the amount to drop (when a reader deletes a data file) before proceeding.
//!
//! ### Ledger structure
//!
//! Likewise, the ledger file consists of a simplified structure that is optimized for being
//! shared via a memory-mapped file interface between the writer and reader.  Like the record
//! structure, the below is a pseudo-structure as we use `rkyv` for the ledger, and so the on-disk
//! layout will be slightly different:
//!
//!   buffer.db:
//!     [total record count - unsigned 64-bit integer]
//!     [total buffer size - unsigned 64-bit integer]
//!     [next record ID - unsigned 64-bit integer]
//!     [writer current data file ID - unsigned 16-bit integer]
//!     [reader current data file ID - unsigned 16-bit integer]
//!     [reader last record ID - unsigned 64-bit integer]
//!
//! As the disk buffer structure is meant to emulate a ring buffer, most of the bookkeeping resolves around the
//! writer and reader being able to quickly figure out where they left off.  Record and data file
//! IDs are simply rolled over when they reach the maximum of their data type, and are incremented
//! monotonically as new data files are created, rather than trying to always allocate from the
//! lowest available ID.
//!
//! Additionally, record IDs are allocated in the same way: monotonic, sequential, and will wrap
//! when they reach the maximum value for the data type.  For record IDs, however, this would mean
//! reaching 2^64, which will take a really, really, really long time.
use std::{marker::PhantomData, sync::Arc};

use snafu::{ResultExt, Snafu};

mod acknowledgements;
mod backed_archive;
mod common;
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
    ledger::LedgerLoadCreateError,
    reader::{Reader, ReaderError},
    writer::{Writer, WriterError},
};
use crate::{buffer_usage_data::BufferUsageHandle, Acker, Bufferable};

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
    pub(crate) async fn from_config_inner(
        config: DiskBufferConfig,
        usage_handle: BufferUsageHandle,
    ) -> Result<(Writer<T>, Reader<T>, Acker, Arc<Ledger>), BufferError<T>> {
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
    /// buffer, respectively, will be returned.  Additionally, an [`Acker`] will be returned, which
    /// must be used to indicate when records read from the [`Reader`] can be considered durably
    /// processed and able to be deleted from the buffer.
    ///
    /// # Errors
    ///
    /// If an error occurred during the creation or loading of the disk buffer, an error variant
    /// will be returned describing the error.
    #[cfg_attr(test, instrument(skip(config, usage_handle), level = "trace"))]
    pub async fn from_config(
        config: DiskBufferConfig,
        usage_handle: BufferUsageHandle,
    ) -> Result<(Writer<T>, Reader<T>, Acker), BufferError<T>> {
        let (writer, reader, acker, _) = Self::from_config_inner(config, usage_handle).await?;

        Ok((writer, reader, acker))
    }
}
