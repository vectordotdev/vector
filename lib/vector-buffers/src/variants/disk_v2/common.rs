use std::{
    cmp,
    path::{Path, PathBuf},
    time::Duration,
};

use crc32fast::Hasher;
use snafu::Snafu;

use super::io::{Filesystem, ProductionFilesystem};

// We don't want data files to be bigger than 128MB, but we might end up overshooting slightly.
pub const DEFAULT_MAX_DATA_FILE_SIZE: usize = 128 * 1024 * 1024;

// We allow records to be as large as a data file.
//
// Practically, this means we'll allow records that are just about as big as as a single data file, but they won't
// _exceed_ the size of a data file, even if they're the first write to a data file.
pub const DEFAULT_MAX_RECORD_SIZE: usize = DEFAULT_MAX_DATA_FILE_SIZE;

// We want to ensure a reasonable time before we `fsync`/flush to disk, and 500ms should provide that for non-critical
// workloads.
//
// Practically, it's far more definitive than `disk_v1` which does not definitvely `fsync` at all, at least with how we
// have it configured.
pub const DEFAULT_FLUSH_INTERVAL: Duration = Duration::from_millis(500);

// Using 256KB as it aligns nicely with the I/O size exposed by major cloud providers.  This may not
// be the underlying block size used by the OS, but it still aligns well with what will happen on
// the "backend" for cloud providers, which is simply a useful default for when we want to look at
// buffer throughput and estimate how many IOPS will be consumed, etc.
pub const DEFAULT_WRITE_BUFFER_SIZE: usize = 256 * 1024;

// We specifically limit ourselves to 0-31 for file IDs in test, because it lets us more quickly
// create/consume the file IDs so we can test edge cases like file ID rollover and "writer is
// waiting to open file that reader is still on".
#[cfg(not(test))]
pub const MAX_FILE_ID: u16 = u16::MAX;
#[cfg(test)]
pub const MAX_FILE_ID: u16 = 6;

pub(crate) fn create_crc32c_hasher() -> Hasher {
    crc32fast::Hasher::new()
}

#[derive(Debug, Snafu)]
pub enum BuildError {
    #[snafu(display("parameter '{}' was invalid: {}", param_name, reason))]
    InvalidParameter {
        param_name: &'static str,
        reason: String,
    },
}

/// Buffer configuration.
#[derive(Clone, Debug)]
pub struct DiskBufferConfig<FS> {
    /// Directory where this buffer will write its files.
    ///
    /// Must be unique from all other buffers, whether within the same process or other Vector
    /// processes on the machine.
    pub(crate) data_dir: PathBuf,

    /// Maximum size, in bytes, that the buffer can consume.
    ///
    /// The actual maximum on-disk buffer size is this amount rounded up to the next multiple of
    /// `max_data_file_size`, but internally, the next multiple of `max_data_file_size` when
    /// rounding this amount _down_ is what gets used as the maximum buffer size.
    ///
    /// This ensures that we never use more then the documented "rounded to the next multiple"
    /// amount, as we must account for one full data file's worth of extra data.
    pub(crate) max_buffer_size: u64,

    /// Maximum size, in bytes, to target for each individual data file.
    ///
    /// This value is not strictly obey because we cannot know ahead of encoding/serializing if the
    /// free space a data file has is enough to hold the write.  In other words, we never attempt to
    /// write to a data file if it is as larger or larger than this value, but may write a record
    /// that causes a data file to exceed this value by as much as `max_record_size`.
    pub(crate) max_data_file_size: u64,

    /// Maximum size, in bytes, of an encoded record.
    ///
    /// Any record which, when encoded, is larger than this amount (with a small caveat, see note)
    /// will not be written to the buffer.
    pub(crate) max_record_size: usize,

    /// Size, in bytes, of the writer's internal buffer.
    ///
    /// This buffer is used to coalesce writes to the underlying data file where possible, which in
    /// turn reduces the number of syscalls needed to issue writes to the underlying data file.
    pub(crate) write_buffer_size: usize,

    /// Flush interval for ledger and data files.
    ///
    /// While data is asynchronously flushed by the OS, and the reader/writer can proceed with a
    /// "hard" flush (aka `fsync`/`fsyncdata`), the flush interval effectively controls the
    /// acceptable window of time for data loss.
    ///
    /// In the event that data had not yet been durably written to disk, and Vector crashed, the
    /// amount of data written since the last flush would be lost.
    pub(crate) flush_interval: Duration,

    /// Filesystem implementation for opening data files.
    ///
    /// We allow parameterizing the filesystem implementation for ease of testing.  The "filesystem"
    /// implementation essentially defines how we open and delete data files, as well as the type of
    /// the data file objects we get when opening a data file.
    pub(crate) filesystem: FS,
}

/// Builder for [`DiskBufferConfig`].
#[derive(Clone, Debug)]
pub struct DiskBufferConfigBuilder<FS = ProductionFilesystem>
where
    FS: Filesystem,
{
    pub(crate) data_dir: PathBuf,
    pub(crate) max_buffer_size: Option<u64>,
    pub(crate) max_data_file_size: Option<u64>,
    pub(crate) max_record_size: Option<usize>,
    pub(crate) write_buffer_size: Option<usize>,
    pub(crate) flush_interval: Option<Duration>,
    pub(crate) filesystem: FS,
}

impl DiskBufferConfigBuilder {
    pub fn from_path<P>(data_dir: P) -> DiskBufferConfigBuilder
    where
        P: AsRef<Path>,
    {
        DiskBufferConfigBuilder {
            data_dir: data_dir.as_ref().to_path_buf(),
            max_buffer_size: None,
            max_data_file_size: None,
            max_record_size: None,
            write_buffer_size: None,
            flush_interval: None,
            filesystem: ProductionFilesystem,
        }
    }
}

impl<FS> DiskBufferConfigBuilder<FS>
where
    FS: Filesystem,
{
    /// Sets the maximum size, in bytes, that the buffer can consume.
    ///
    /// The actual maximum on-disk buffer size is this amount rounded up to the next multiple of
    /// `max_data_file_size`, but internally, the next multiple of `max_data_file_size` when
    /// rounding this amount _down_ is what gets used as the maximum buffer size.
    ///
    /// This ensures that we never use more then the documented "rounded to the next multiple"
    /// amount, as we must account for one full data file's worth of extra data.
    ///
    /// Defaults to `usize::MAX`, or effectively no limit.  Due to the internal design of the
    /// buffer, the effective maximum limit is around `max_data_file_size` * 2^16.
    #[allow(dead_code)]
    pub fn max_buffer_size(mut self, amount: u64) -> Self {
        self.max_buffer_size = Some(amount);
        self
    }

    /// Sets the maximum size, in bytes, to target for each individual data file.
    ///
    /// This value is not strictly obey because we cannot know ahead of encoding/serializing if the
    /// free space a data file has is enough to hold the write.  In other words, we never attempt to
    /// write to a data file if it is as larger or larger than this value, but may write a record
    /// that causes a data file to exceed this value by as much as `max_record_size`.
    ///
    /// Defaults to 128MB.
    #[allow(dead_code)]
    pub fn max_data_file_size(mut self, amount: u64) -> Self {
        self.max_data_file_size = Some(amount);
        self
    }

    /// Sets the maximum size, in bytes, of an encoded record.
    ///
    /// Any record which, when encoded, is larger than this amount (with a small caveat, see note)
    /// will not be written to the buffer.
    ///
    /// Defaults to 128MB.
    #[allow(dead_code)]
    pub fn max_record_size(mut self, amount: usize) -> Self {
        self.max_record_size = Some(amount);
        self
    }

    /// Size, in bytes, of the writer's internal buffer.
    ///
    /// This buffer is used to coalesce writes to the underlying data file where possible, which in
    /// turn reduces the number of syscalls needed to issue writes to the underlying data file.
    ///
    /// Defaults to 256KB.
    #[allow(dead_code)]
    pub fn write_buffer_size(mut self, amount: usize) -> Self {
        self.write_buffer_size = Some(amount);
        self
    }

    /// Sets the flush interval for ledger and data files.
    ///
    /// While data is asynchronously flushed by the OS, and the reader/writer can proceed with a
    /// "hard" flush (aka `fsync`/`fsyncdata`), the flush interval effectively controls the
    /// acceptable window of time for data loss.
    ///
    /// In the event that data had not yet been durably written to disk, and Vector crashed, the
    /// amount of data written since the last flush would be lost.
    ///
    /// Defaults to 500ms.
    #[allow(dead_code)]
    pub fn flush_interval(mut self, interval: Duration) -> Self {
        self.flush_interval = Some(interval);
        self
    }

    /// Filesystem implementation for opening data files.
    ///
    /// We allow parameterizing the filesystem implementation for ease of testing.  The "filesystem"
    /// implementation essentially defines how we open and delete data files, as well as the type of
    /// the data file objects we get when opening a data file.
    ///
    /// Defaults to a Tokio-backed implementation.
    #[allow(dead_code)]
    pub fn filesystem<FS2>(self, filesystem: FS2) -> DiskBufferConfigBuilder<FS2>
    where
        FS2: Filesystem,
    {
        DiskBufferConfigBuilder {
            data_dir: self.data_dir,
            max_buffer_size: self.max_buffer_size,
            max_data_file_size: self.max_data_file_size,
            max_record_size: self.max_record_size,
            write_buffer_size: self.write_buffer_size,
            flush_interval: self.flush_interval,
            filesystem,
        }
    }

    /// Consumes this builder and constructs a `DiskBufferConfig`.
    pub fn build(self) -> Result<DiskBufferConfig<FS>, BuildError> {
        let max_buffer_size = self.max_buffer_size.unwrap_or(u64::MAX);
        let max_data_file_size = self.max_data_file_size.unwrap_or_else(|| {
            u64::try_from(DEFAULT_MAX_DATA_FILE_SIZE)
                .expect("Vector does not support 128-bit platforms.")
        });
        let max_record_size = self.max_record_size.unwrap_or(DEFAULT_MAX_RECORD_SIZE);
        let write_buffer_size = self.write_buffer_size.unwrap_or(DEFAULT_WRITE_BUFFER_SIZE);
        let flush_interval = self.flush_interval.unwrap_or(DEFAULT_FLUSH_INTERVAL);
        let filesystem = self.filesystem;

        // Validate the input parameters.
        if max_data_file_size == 0 {
            return Err(BuildError::InvalidParameter {
                param_name: "max_data_file_size",
                reason: "cannot be zero".to_string(),
            });
        }

        if max_buffer_size < max_data_file_size {
            return Err(BuildError::InvalidParameter {
                param_name: "max_buffer_size",
                reason: format!(
                    "must be greater than or equal to {} bytes",
                    max_data_file_size
                ),
            });
        }

        if max_record_size == 0 {
            return Err(BuildError::InvalidParameter {
                param_name: "max_record_size",
                reason: "cannot be zero".to_string(),
            });
        }

        if write_buffer_size == 0 {
            return Err(BuildError::InvalidParameter {
                param_name: "write_buffer_size",
                reason: "cannot be zero".to_string(),
            });
        }

        // We calculate our current buffer size based on the number of unacknowledged records. However, we only delete
        // data files once they've been entirely acknowledged. This means that we may report a current buffer size that
        // is smaller than the sum of the size of the data files currently on disk.
        //
        // We do this because if we used the true on-disk total buffer size, we might stall further writes until an
        // entire data file was deleted, even if we had fully acknowledged all but the last record in a data file, etc.
        // We essentially trade off using up to an additional `DEFAULT_MAX_DATA_FILE_SIZE` bytes to allow writers to
        // make progress as records are read and acknowledged, when the buffer is riding close to the overall buffer
        // size limit.
        //
        // In practical terms, this means the expected true on-disk total buffer size can grow to `max_buffer_size`,
        // rounded up to the closest multiple of `DEFAULT_MAX_DATA_FILE_SIZE`. On the flipside, though, we limit our
        // functional max buffer size -- the value we use internally for limiting writes until more records are
        // acknowledged, etc -- to `max_buffer_size` rounded _down_ to the closest multiple of
        // `DEFAULT_MAX_DATA_FILE_SIZE`.
        let max_buffer_size = max_buffer_size - (max_buffer_size % max_data_file_size);
        let max_buffer_size = cmp::max(max_buffer_size, max_data_file_size);

        Ok(DiskBufferConfig {
            data_dir: self.data_dir,
            max_buffer_size,
            max_data_file_size,
            max_record_size,
            write_buffer_size,
            flush_interval,
            filesystem,
        })
    }
}

#[cfg(test)]
mod tests {
    use proptest::{prop_assert, prop_assert_eq, proptest};

    use crate::variants::disk_v2::{common::DEFAULT_MAX_DATA_FILE_SIZE, DiskBufferConfigBuilder};

    const MAX_BUFFER_SIZE_TEST_UPPER_BOUND_INPUT: usize = DEFAULT_MAX_DATA_FILE_SIZE * 10;

    proptest! {
        #[test]
        fn ensure_max_disk_buffer_size_lower_bound(max_buffer_size in DEFAULT_MAX_DATA_FILE_SIZE..MAX_BUFFER_SIZE_TEST_UPPER_BOUND_INPUT) {
            // This is a little ugly but the `u64`/`usize` conversions make it annoying to do in a full const way.
            let default_max_data_file_size = u64::try_from(DEFAULT_MAX_DATA_FILE_SIZE)
                .expect("`DEFAULT_MAX_DATA_FILE_SIZE` should not ever be greater than `u64::MAX`.");
            let max_buffer_size = u64::try_from(max_buffer_size)
                .expect("`max_buffer_size` should not ever be greater than `u64::MAX`.");

            let config = DiskBufferConfigBuilder::from_path("/tmp/dummy/path")
                .max_buffer_size(max_buffer_size)
                .build()
                .expect("errors during the config build are all invalid here");

            // Small sanity check to make sure our logic for enforcing the maximum buffer size doesn't truncate to zero.
            prop_assert!(config.max_buffer_size != 0);

            prop_assert_eq!(config.max_data_file_size, default_max_data_file_size, "the default max data file size should always match the default in this test");
            prop_assert!(config.max_buffer_size >= default_max_data_file_size, "max_buffer_size should be at least as big as max data file size ({}), got {}", default_max_data_file_size, config.max_buffer_size);
        }
    }
}
