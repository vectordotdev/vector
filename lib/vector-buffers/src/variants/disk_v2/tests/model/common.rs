use std::time::Duration;

use proptest::{arbitrary::any, strategy::Strategy};

use super::{filesystem::TestFilesystem, record::Record};
use crate::variants::disk_v2::{
    BufferReader, BufferWriter, DiskBufferConfig, DiskBufferConfigBuilder, ReaderError, WriterError,
};

pub type TestReader = BufferReader<Record, TestFilesystem>;
pub type TestWriter = BufferWriter<Record, TestFilesystem>;
pub type ReaderResult<T> = Result<T, ReaderError<Record>>;
pub type WriterResult<T> = Result<T, WriterError<Record>>;

// This is specifically set at 60KB because we allow a maximum record size of up to 64KB, and so
// we'd likely to occasionally encounter a record that, when encoded, is larger than the write
// buffer overall, which exercises the "write this record directly to the wrapped writer" logic that
// exists in `tokio::io::BufWriter` itself.
pub const TEST_WRITE_BUFFER_SIZE: usize = 60 * 1024;

/// Result of applying an action to the model.
///
/// The model can represent asynchronous computation, so progress can be both the completion of an
/// operation, whether success or failure, as well as the indication of a lack of completion
/// (blocked, or pending in future parlance).
#[derive(Debug, PartialEq)]
pub enum Progress {
    RecordWritten(usize),
    WriteError(WriterError<Record>),
    RecordRead(Option<Record>),
    ReadError(ReaderError<Record>),
    Blocked,
}

pub fn arb_buffer_config() -> impl Strategy<Value = DiskBufferConfig<TestFilesystem>> {
    any::<(u16, u16, u16)>()
        .prop_map(|(n1, n2, n3)| {
            let max_buffer_size = u64::from(n1) * 64;
            let max_data_file_size = u64::from(n2) * 2;
            let max_record_size = n3.into();

            let mut path = std::env::temp_dir();
            path.push("vector-disk-v2-model");

            DiskBufferConfigBuilder::from_path(path)
                .max_buffer_size(max_buffer_size)
                .max_data_file_size(max_data_file_size)
                .max_record_size(max_record_size)
                .write_buffer_size(TEST_WRITE_BUFFER_SIZE)
                // This really only affects how often we flush the ledger, because we always `flush`
                // after writes to ensure our buffered writes make it to the data files for the
                // readers to make progress, and we're not testing anything about whether or not the
                // ledger makes it to disk durably.
                .flush_interval(Duration::from_secs(10))
                .filesystem(TestFilesystem::default())
        })
        .prop_filter_map(
            "maximum size limits were too high, or zero",
            validate_buffer_config,
        )
}

/// Validates the given buffer config builder and generates a resulting configuration.
///
/// If the builder has been configured incorrectly (i.e. zero values), or if the configuration is
/// valid but has values that are not appropriate for being used under test (i.e. values are way too
/// large and would balloon the run-time of the test) then `None` is returned.
///
/// Otherwise, `Some(DiskBufferConfig)` is returned.
pub fn validate_buffer_config(
    builder: DiskBufferConfigBuilder<TestFilesystem>,
) -> Option<DiskBufferConfig<TestFilesystem>> {
    builder
        .build()
        // If building the configuration failed, just return `None`.
        .ok()
        .filter(|config| {
            // Limit our buffer config to the following:
            // - max buffer size of 64MB
            // - max data file size of 2MB
            // - max record size of 1MB
            //
            // Otherwise, the model just runs uselessly slow.
            config.max_buffer_size <= 64_000_000
                && config.max_data_file_size <= 2_000_000
                && config.max_record_size <= 1_000_000
        })
}
