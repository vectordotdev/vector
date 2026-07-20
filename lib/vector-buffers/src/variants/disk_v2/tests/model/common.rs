use std::time::Duration;

use proptest::strategy::{Just, Strategy};

use super::{
    filesystem::{TestFilesystem, arb_fs_atomicity},
    record::Record,
};
use crate::variants::disk_v2::{
    BufferReader, BufferWriter, DiskBufferConfigBuilder, ReaderError, WriterError,
    common::MINIMUM_MAX_RECORD_SIZE, ledger::LEDGER_LEN,
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
const MODEL_MAX_RAW_BUFFER_SIZE: u64 = 4_194_240;
const MODEL_MAX_DATA_FILE_SIZE: u64 = 131_070;
const MODEL_MAX_RECORD_SIZE: usize = 65_535;
const MIN_VALID_MAX_RECORD_SIZE: usize = MINIMUM_MAX_RECORD_SIZE + 1;

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

pub fn arb_buffer_config() -> impl Strategy<Value = DiskBufferConfigBuilder<TestFilesystem>> {
    // Generate dependent limits directly as minimum + slack, rather than generating invalid
    // triples and rejecting them, so each max_* field has a clean path toward its lower bound.
    (
        MIN_VALID_MAX_RECORD_SIZE..=MODEL_MAX_RECORD_SIZE,
        1u64..=60,
        arb_fs_atomicity(),
    )
        .prop_flat_map(|(max_record_size, flush_interval_secs, atomicity)| {
            let min_data_file_size =
                u64::try_from(max_record_size).expect("model max record size must fit in u64");
            let max_data_file_size_slack = MODEL_MAX_DATA_FILE_SIZE - min_data_file_size;

            (
                Just((max_record_size, flush_interval_secs, atomicity)),
                0u64..=max_data_file_size_slack,
            )
        })
        .prop_flat_map(
            |((max_record_size, flush_interval_secs, atomicity), max_data_file_size_slack)| {
                let min_data_file_size =
                    u64::try_from(max_record_size).expect("model max record size must fit in u64");
                let max_data_file_size = min_data_file_size + max_data_file_size_slack;
                let min_buffer_size = minimum_raw_buffer_size(max_data_file_size);
                let max_buffer_size_slack = MODEL_MAX_RAW_BUFFER_SIZE - min_buffer_size;

                (
                    Just((
                        max_record_size,
                        max_data_file_size,
                        flush_interval_secs,
                        atomicity,
                    )),
                    0u64..=max_buffer_size_slack,
                )
            },
        )
        .prop_map(
            |(
                (max_record_size, max_data_file_size, flush_interval_secs, atomicity),
                max_buffer_size_slack,
            )| {
                let max_buffer_size =
                    minimum_raw_buffer_size(max_data_file_size) + max_buffer_size_slack;
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
                    .flush_interval(Duration::from_secs(flush_interval_secs))
                    .filesystem(TestFilesystem::with_atomicity(atomicity))
            },
        )
}

fn minimum_raw_buffer_size(max_data_file_size: u64) -> u64 {
    let ledger_len = u64::try_from(LEDGER_LEN).expect("ledger length must fit in u64");

    max_data_file_size
        .checked_mul(2)
        .and_then(|doubled| doubled.checked_add(ledger_len))
        .expect("model max data file size must leave room for the minimum buffer size")
}
