//! # Disk Buffer v2: Sequential File I/O Boogaloo.
//!
//! This disk buffer implementation is a reimplementation of the LevelDB-based disk buffer code that
//! already exists, but seeks to increase performance and reliability, while reducing the amount of
//! external code and hard-to-expose tunables.
//!
//! ## Design constraints
//!
//! These constraints, or more often, invariants, are the groundwork for ensuring that the design
//! can stay simple and understandable:
//! - buffer can grow to a maximum of ~8TB in total size
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
//!     record_len: uint32
//!     checksum: uint32 (CRC32C of record_id + payload)
//!     record_id: uint64
//!     payload: uint8[]
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
//!
//! # Implementation TODOs:
//!
//! - make file size limits configurable for testing purposes (we could easily write 2-3x of the
//!   128MB target under test, but it'd be faster if we didn't have to, and doing that would take a
//!   while to exercise certain logic like file ID wraparound)
//! - actually limit the total file usage size (add logic to update the total file size in the ledger)
//! - test what happens on file ID rollover
//! - test what happens on writer wrapping and wanting to open current data file being read by reader
//! - test what happens when record ID rolls over
use std::{marker::PhantomData, path::Path, sync::Arc, time::Duration};

use snafu::{ResultExt, Snafu};

mod backed_archive;
mod common;
mod ledger;
mod reader;
mod record;
mod ser;
mod writer;

use crate::Bufferable;

use self::{
    common::BufferConfig,
    ledger::{Ledger, LedgerLoadCreateError},
    reader::{Reader, ReaderError},
    writer::{Writer, WriterError},
};

#[derive(Debug, Snafu)]
pub enum BufferError<T>
where
    T: Bufferable,
{
    #[snafu(display("failed to load/create ledger: {}", source))]
    LedgerError { source: LedgerLoadCreateError },
    #[snafu(display("failed to seek to position where reader left off: {}", source))]
    ReaderSeekFailed { source: ReaderError<T> },
    #[snafu(display("failed to seek to position where writer left off: {}", source))]
    WriterSeekFailed { source: WriterError<T> },
}

pub struct Buffer<T> {
    _t: PhantomData<T>,
}

impl<T> Buffer<T>
where
    T: Bufferable,
{
    pub async fn from_config(
        config: BufferConfig,
    ) -> Result<(Writer<T>, Reader<T>), BufferError<T>> {
        let ledger = Ledger::load_or_create(config).await.context(LedgerError)?;
        let ledger = Arc::new(ledger);

        let mut writer = Writer::new(Arc::clone(&ledger));
        writer
            .validate_last_write()
            .await
            .context(WriterSeekFailed)?;

        let mut reader = Reader::new(ledger);
        reader
            .seek_to_next_record()
            .await
            .context(ReaderSeekFailed)?;

        Ok((writer, reader))
    }
}
