use std::{
    io::{self, Cursor},
    path::Path,
    sync::Arc,
};

use tokio::{
    fs::OpenOptions,
    io::{AsyncWriteExt, DuplexStream},
};

use super::{
    io::{AsyncFile, Metadata, ProductionFilesystem, ReadableMemoryMap, WritableMemoryMap},
    ledger::LEDGER_LEN,
    record::RECORD_HEADER_LEN,
    Buffer, BufferReader, BufferWriter, DiskBufferConfigBuilder, Filesystem, Ledger,
};
use crate::{
    buffer_usage_data::BufferUsageHandle, encoding::FixedEncodable,
    variants::disk_v2::common::align16, Bufferable,
};

type FilesystemUnderTest = ProductionFilesystem;

mod acknowledgements;
mod basic;
mod initialization;
mod invariants;
mod known_errors;
mod model;
mod record;
mod size_limits;

impl AsyncFile for DuplexStream {
    async fn metadata(&self) -> io::Result<Metadata> {
        Ok(Metadata { len: 0 })
    }

    async fn sync_all(&self) -> io::Result<()> {
        Ok(())
    }
}

impl AsyncFile for Cursor<Vec<u8>> {
    async fn metadata(&self) -> io::Result<Metadata> {
        Ok(Metadata { len: 0 })
    }

    async fn sync_all(&self) -> io::Result<()> {
        Ok(())
    }
}

impl ReadableMemoryMap for Vec<u8> {}

impl WritableMemoryMap for Vec<u8> {
    fn flush(&self) -> io::Result<()> {
        Ok(())
    }
}

#[macro_export]
macro_rules! assert_buffer_is_empty {
    ($ledger:expr) => {
        assert_eq!(
            $ledger.get_total_records(),
            0,
            "ledger should have 0 records, but had {}",
            $ledger.get_total_records()
        );
        assert_eq!(
            $ledger.get_total_buffer_size(),
            0,
            "ledger should have 0 bytes, but had {} bytes",
            $ledger.get_total_buffer_size()
        );
    };
}

#[macro_export]
macro_rules! assert_buffer_records {
    ($ledger:expr, $record_count:expr) => {
        assert_eq!(
            $ledger.get_total_records(),
            u64::try_from($record_count).expect("Record count is out of range"),
            "ledger should have {} records, but had {}",
            $record_count,
            $ledger.get_total_records()
        );
    };
}

#[macro_export]
macro_rules! assert_buffer_size {
    ($ledger:expr, $record_count:expr, $buffer_size:expr) => {
        assert_eq!(
            $ledger.get_total_records(),
            u64::try_from($record_count).expect("Record count is out of range"),
            "ledger should have {} records, but had {}",
            $record_count,
            $ledger.get_total_records()
        );
        assert_eq!(
            $ledger.get_total_buffer_size(),
            u64::try_from($buffer_size).expect("Buffer size is out of range"),
            "ledger should have {} bytes, but had {} bytes",
            $buffer_size,
            $ledger.get_total_buffer_size()
        );
    };
}

#[macro_export]
macro_rules! assert_reader_writer_v2_file_positions {
    ($ledger:expr, $reader:expr, $writer:expr) => {{
        let (reader, writer) = $ledger.get_current_reader_writer_file_id();
        assert_eq!(
            u16::try_from($reader).expect("Reader value is out of range"),
            reader,
            "expected reader file ID of {}, got {} instead",
            ($reader),
            reader
        );
        assert_eq!(
            u16::try_from($writer).expect("Writer value is out of range"),
            writer,
            "expected writer file ID of {}, got {} instead",
            ($writer),
            writer
        );
    }};
}

#[macro_export]
macro_rules! assert_reader_last_writer_next_positions {
    ($ledger:expr, $reader_expected:expr, $writer_expected:expr) => {{
        let reader_actual = $ledger.state().get_last_reader_record_id();
        let writer_actual = $ledger.state().get_next_writer_record_id();
        assert_eq!(
            $reader_expected, reader_actual,
            "expected reader last read record ID of {}, got {} instead",
            $reader_expected, reader_actual,
        );
        assert_eq!(
            $writer_expected, writer_actual,
            "expected writer next record ID of {}, got {} instead",
            $writer_expected, writer_actual,
        );
    }};
}

#[macro_export]
macro_rules! assert_enough_bytes_written {
    ($written:expr, $record_type:ty, $record_payload_size:expr) => {
        assert!(
            $written >= $record_payload_size as usize + 8 + std::mem::size_of::<$record_type>()
        );
    };
}

#[macro_export]
macro_rules! set_data_file_length {
    ($path:expr, $start_len:expr, $target_len:expr) => {{
        let mut data_file = OpenOptions::new()
            .write(true)
            .open(&$path)
            .await
            .expect("open should not fail");

        // Just to make sure the data file matches our expected state before futzing with it.
        let metadata = data_file
            .metadata()
            .await
            .expect("metadata should not fail");
        assert_eq!(
            ($start_len) as u64,
            metadata.len(),
            "expected data file to be {} bytes long, but was actually {} bytes long",
            ($start_len) as u64,
            metadata.len()
        );

        data_file
            .set_len(($target_len) as u64)
            .await
            .expect("truncate should not fail");
        data_file.flush().await.expect("flush should not fail");
        data_file.sync_all().await.expect("sync should not fail");
        drop(data_file);
    }};
}

/// Creates a disk v2 buffer with all default values i.e. maximum buffer size, etc.
pub(crate) async fn create_default_buffer_v2<P, R>(
    data_dir: P,
) -> (
    BufferWriter<R, FilesystemUnderTest>,
    BufferReader<R, FilesystemUnderTest>,
    Arc<Ledger<FilesystemUnderTest>>,
)
where
    P: AsRef<Path>,
    R: Bufferable,
{
    let config = DiskBufferConfigBuilder::from_path(data_dir)
        .build()
        .expect("creating buffer should not fail");
    let usage_handle = BufferUsageHandle::noop();
    Buffer::from_config_inner(config, usage_handle)
        .await
        .expect("should not fail to create buffer")
}

/// Creates a disk v2 buffer with all default values, but returns a handle to the buffer usage tracker.
pub(crate) async fn create_default_buffer_v2_with_usage<P, R>(
    data_dir: P,
) -> (
    BufferWriter<R, FilesystemUnderTest>,
    BufferReader<R, FilesystemUnderTest>,
    Arc<Ledger<FilesystemUnderTest>>,
    BufferUsageHandle,
)
where
    P: AsRef<Path>,
    R: Bufferable,
{
    let config = DiskBufferConfigBuilder::from_path(data_dir)
        .build()
        .expect("creating buffer should not fail");
    let usage_handle = BufferUsageHandle::noop();
    let (writer, reader, ledger) = Buffer::from_config_inner(config, usage_handle.clone())
        .await
        .expect("should not fail to create buffer");
    (writer, reader, ledger, usage_handle)
}

/// Creates a disk v2 buffer that is sized such that only a fixed number of data files are allowed.
///
/// We do this based on limiting the maximum buffer size, knowing that if the maximum data file size is N, and we want
/// to limit ourselves to M data files, the maximum buffer size should be N*M. We additionally constrain our maximum
/// record size to the maximum data file size in order to satisfy the configuration builder.
pub(crate) async fn create_buffer_v2_with_data_file_count_limit<P, R>(
    data_dir: P,
    max_data_file_size: u64,
    data_file_count_limit: u64,
) -> (
    BufferWriter<R, FilesystemUnderTest>,
    BufferReader<R, FilesystemUnderTest>,
    Arc<Ledger<FilesystemUnderTest>>,
)
where
    P: AsRef<Path>,
    R: Bufferable,
{
    // We do this here, despite the fact that configuration builder also implicitly does it, because our error message
    // can be more pointed given that we're running tests, whereas the user-visible error message is just about getting
    // them to set a valid amount without needing to understand the internals.
    assert!(
        data_file_count_limit >= 2,
        "data file count limit must be at least 2"
    );

    let max_record_size = usize::try_from(max_data_file_size).unwrap();

    // We also have to compensate for the size of the ledger itself, as the configuration builder pays attention to that
    // in the context of the configured maximum buffer size.
    let ledger_len: u64 = LEDGER_LEN.try_into().unwrap();
    let max_buffer_size = max_data_file_size
        .checked_mul(data_file_count_limit)
        .and_then(|n| n.checked_add(ledger_len))
        .unwrap();

    let config = DiskBufferConfigBuilder::from_path(data_dir)
        .max_record_size(max_record_size)
        .max_data_file_size(max_data_file_size)
        .max_buffer_size(max_buffer_size)
        .build()
        .expect("creating buffer should not fail");
    let usage_handle = BufferUsageHandle::noop();

    Buffer::from_config_inner(config, usage_handle)
        .await
        .expect("should not fail to create buffer")
}

/// Creates a disk v2 buffer with the specified maximum record size.
pub(crate) async fn create_buffer_v2_with_max_record_size<P, R>(
    data_dir: P,
    max_record_size: usize,
) -> (
    BufferWriter<R, FilesystemUnderTest>,
    BufferReader<R, FilesystemUnderTest>,
    Arc<Ledger<FilesystemUnderTest>>,
)
where
    P: AsRef<Path>,
    R: Bufferable,
{
    let config = DiskBufferConfigBuilder::from_path(data_dir)
        .max_record_size(max_record_size)
        .build()
        .expect("creating buffer should not fail");
    let usage_handle = BufferUsageHandle::noop();

    Buffer::from_config_inner(config, usage_handle)
        .await
        .expect("should not fail to create buffer")
}

/// Creates a disk v2 buffer with the specified maximum data file size.
///
/// We additionally constrain our maximum record size to the maximum data file size in order to satisfy the configuration builder.
pub(crate) async fn create_buffer_v2_with_max_data_file_size<P, R>(
    data_dir: P,
    max_data_file_size: u64,
) -> (
    BufferWriter<R, FilesystemUnderTest>,
    BufferReader<R, FilesystemUnderTest>,
    Arc<Ledger<FilesystemUnderTest>>,
)
where
    P: AsRef<Path>,
    R: Bufferable,
{
    let max_record_size = usize::try_from(max_data_file_size).unwrap();

    let config = DiskBufferConfigBuilder::from_path(data_dir)
        .max_data_file_size(max_data_file_size)
        .max_record_size(max_record_size)
        .build()
        .expect("creating buffer should not fail");
    let usage_handle = BufferUsageHandle::noop();

    Buffer::from_config_inner(config, usage_handle)
        .await
        .expect("should not fail to create buffer")
}

/// Creates a disk v2 buffer with the specified write buffer size.
pub(crate) async fn create_buffer_v2_with_write_buffer_size<P, R>(
    data_dir: P,
    write_buffer_size: usize,
) -> (
    BufferWriter<R, FilesystemUnderTest>,
    BufferReader<R, FilesystemUnderTest>,
    Arc<Ledger<FilesystemUnderTest>>,
)
where
    P: AsRef<Path>,
    R: Bufferable,
{
    let config = DiskBufferConfigBuilder::from_path(data_dir)
        .write_buffer_size(write_buffer_size)
        .build()
        .expect("creating buffer should not fail");
    let usage_handle = BufferUsageHandle::noop();

    Buffer::from_config_inner(config, usage_handle)
        .await
        .expect("should not fail to create buffer")
}

pub(crate) fn get_corrected_max_record_size<T>(payload: &T) -> usize
where
    T: FixedEncodable,
{
    let payload_len = payload
        .encoded_size()
        .expect("All test record types must return a valid encoded size.");
    let total = RECORD_HEADER_LEN + payload_len;

    align16(total)
}

pub(crate) fn get_minimum_data_file_size_for_record_payload<T>(payload: &T) -> u64
where
    T: FixedEncodable,
{
    // This is just the maximum record size, compensating for the record header length.
    let max_record_size = get_corrected_max_record_size(payload);
    u64::try_from(max_record_size).unwrap()
}

pub(crate) async fn read_next<T, FS>(reader: &mut BufferReader<T, FS>) -> Option<T>
where
    T: Bufferable,
    FS: Filesystem,
    FS::File: Unpin,
{
    reader.next().await.expect("read should not fail")
}

pub(crate) async fn read_next_some<T, FS>(reader: &mut BufferReader<T, FS>) -> T
where
    T: Bufferable,
    FS: Filesystem,
    FS::File: Unpin,
{
    read_next(reader)
        .await
        .expect("read should produce a record")
}

pub(crate) async fn set_file_length<P: AsRef<Path>>(
    path: P,
    initial_len: u64,
    target_len: u64,
) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .open(&path)
        .await
        .expect("open should not fail");

    // Just to make sure the file matches the expected starting length before futzing with it.
    let metadata = file.metadata().await.expect("metadata should not fail");
    assert_eq!(initial_len, metadata.len());

    file.set_len(target_len)
        .await
        .expect("set_len should not fail");
    file.flush().await.expect("flush should not fail");
    file.sync_all().await.expect("sync should not fail");
    drop(file);

    Ok(())
}
