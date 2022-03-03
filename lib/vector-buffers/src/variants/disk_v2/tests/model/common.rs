use std::{num::NonZeroU16, time::Duration};

use proptest::{arbitrary::any, strategy::Strategy};
use quickcheck::Arbitrary;

use crate::variants::disk_v2::{
    DiskBufferConfig, DiskBufferConfigBuilder, Reader, ReaderError, Writer, WriterError,
};

use super::{filesystem::TestFilesystem, record::Record};

pub type TestReader = Reader<Record, TestFilesystem>;
pub type TestWriter = Writer<Record, TestFilesystem>;
pub type ReaderResult<T> = Result<T, ReaderError<Record>>;
pub type WriterResult<T> = Result<T, WriterError<Record>>;

// This is specifically set at 60KB because we allow a maximum record size of up to 64KB, and so
// we'd likely to occassionally encounter a record that, when encoded, is larger than the write
// buffer overall, which exercises the "write this record directly to the wrapped writer" logic that
// exists in `tokio::io::BufWriter` itself.
pub const TEST_WRITE_BUFFER_SIZE: u64 = 60 * 1024;

/// Result of applying an action to the model.
///
/// The model can represent asynchronous computation, so progress can be both the completion of an
/// operation, whether success or failure, as well as the indication of a lack of completion
/// (blocked, or pending in future parlance).
#[derive(Debug, PartialEq)]
pub enum Progress {
    RecordWritten(usize),
    WriteError,
    RecordRead(Option<Record>),
    ReadError,
    Blocked,
}

#[macro_export]
macro_rules! quickcheck_assert_eq {
    ($expected:expr, $actual:expr, $reason:expr) => {{
        if $expected != $actual {
            return TestResult::error($reason);
        }
    }};
}

impl Arbitrary for DiskBufferConfigBuilder<TestFilesystem> {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        // We limit our buffer size, data file size, and record size to make sure that quickcheck
        // can actually make some reasonable progress.  We do this in a ratio of 64 to 2 to 1,
        // respectively.  This should allow exercising the file ID limit in test mode (32 data files
        // max) but not constantly hitting it.
        let max_buffer_size = NonZeroU16::arbitrary(g).get() as u64 * 64;
        let max_data_file_size = NonZeroU16::arbitrary(g).get() as u64 * 2;
        let max_record_size = NonZeroU16::arbitrary(g).get() as usize;

        let mut path = std::env::temp_dir();
        path.push("vector-disk-v2-model");

        DiskBufferConfigBuilder::from_path(path)
            .max_buffer_size(max_buffer_size)
            .max_data_file_size(max_data_file_size)
            .max_record_size(max_record_size)
            .write_buffer_size(TEST_WRITE_BUFFER_SIZE as usize)
            .flush_interval(Duration::arbitrary(g))
            .filesystem(TestFilesystem::default())
    }
}

pub fn arb_buffer_config() -> impl Strategy<Value = DiskBufferConfig<TestFilesystem>> {
    any::<(u16, u16, u16)>()
        .prop_filter(
            "buffer configuration values cannot be zero".to_owned(),
            |(n1, n2, n3)| *n1 != 0 && *n2 != 0 && *n3 != 0,
        )
        .prop_map(|(n1, n2, n3)| {
            let max_buffer_size = n1 as u64 * 64;
            let max_data_file_size = n2 as u64 * 2;
            let max_record_size = n3 as usize;

            let mut path = std::env::temp_dir();
            path.push("vector-disk-v2-model");

            DiskBufferConfigBuilder::from_path(path)
                .max_buffer_size(max_buffer_size)
                .max_data_file_size(max_data_file_size)
                .max_record_size(max_record_size)
                .write_buffer_size(TEST_WRITE_BUFFER_SIZE as usize)
                // This really only affects how often we flush the ledger, because we always `flush`
                // after writes to ensure our buffered writes make it to the data files for the
                // readers to make progress, and we're not testing anything about whether or not the
                // ledger makes it to disk durably.
                .flush_interval(Duration::from_secs(10))
                .filesystem(TestFilesystem::default())
                .build()
                .expect("config should not fail to build")
        })
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
