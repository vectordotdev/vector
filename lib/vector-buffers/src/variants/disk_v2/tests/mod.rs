use std::{
    io::{self, Cursor},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use tokio::{
    fs::OpenOptions,
    io::{AsyncWriteExt, DuplexStream},
};

use super::{
    Buffer, BufferReader, BufferWriter, DiskBufferConfigBuilder, Filesystem, Ledger,
    io::{AsyncFile, Metadata, ProductionFilesystem, ReadableMemoryMap, WritableMemoryMap},
    ledger::LEDGER_LEN,
    record::RECORD_HEADER_LEN,
};
use crate::{
    Bufferable, buffer_usage_data::BufferUsageHandle, encoding::FixedEncodable,
    test::with_temp_dir, variants::disk_v2::common::align16,
};

type FilesystemUnderTest = ProductionFilesystem;

struct WriteGateCleanup(Arc<super::io::TestWriteGate>);

impl Drop for WriteGateCleanup {
    fn drop(&mut self) {
        self.0.release();
    }
}

async fn open_production_ledger(
    data_dir: &Path,
    filesystem: ProductionFilesystem,
) -> Result<Ledger<ProductionFilesystem>, super::ledger::LedgerLoadCreateError> {
    let config = DiskBufferConfigBuilder::from_path(data_dir)
        .filesystem(filesystem)
        .build()
        .unwrap();
    Ledger::load_or_create(config, BufferUsageHandle::noop()).await
}

async fn reopen_production_ledger(
    data_dir: &Path,
) -> Result<Ledger<ProductionFilesystem>, super::ledger::LedgerLoadCreateError> {
    open_production_ledger(data_dir, ProductionFilesystem::default()).await
}

async fn assert_session_locked(data_dir: &Path) {
    assert!(matches!(
        reopen_production_ledger(data_dir).await,
        Err(super::ledger::LedgerLoadCreateError::LedgerLockAlreadyHeld)
    ));
}

async fn wait_for_session_unlock(data_dir: &Path) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match reopen_production_ledger(data_dir).await {
                Ok(_) => break,
                Err(super::ledger::LedgerLoadCreateError::LedgerLockAlreadyHeld) => {
                    tokio::task::yield_now().await;
                }
                Err(error) => panic!("buffer reopen failed: {error}"),
            }
        }
    })
    .await
    .expect("session lock should be released after the detached write completes");
}

mod acknowledgements;
mod basic;
mod filter_metrics;
mod initialization;
mod invariants;
mod known_errors;
pub(crate) mod model;
mod record;
mod runtime_capacity;
mod size_limits;

impl AsyncFile for DuplexStream {
    async fn metadata(&self) -> io::Result<Metadata> {
        Ok(Metadata { len: 0 })
    }

    async fn truncate(&self, size: u64) -> io::Result<()> {
        if size == 0 {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot extend a file through the truncation API",
            ))
        }
    }

    async fn sync_all(&self) -> io::Result<()> {
        Ok(())
    }
}

impl AsyncFile for Cursor<Vec<u8>> {
    async fn metadata(&self) -> io::Result<Metadata> {
        Ok(Metadata {
            len: u64::try_from(self.get_ref().len()).expect("cursor length should fit in u64"),
        })
    }

    async fn truncate(&self, size: u64) -> io::Result<()> {
        if size > self.metadata().await?.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot extend a file through the truncation API",
            ));
        }

        Ok(())
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

/// Creates a disk v2 buffer with the specified maximum record size, but returns a handle to the
/// buffer usage tracker.
pub(crate) async fn create_buffer_v2_with_max_record_size_and_usage<P, R>(
    data_dir: P,
    max_record_size: usize,
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
        .max_record_size(max_record_size)
        .build()
        .expect("creating buffer should not fail");
    let usage_handle = BufferUsageHandle::noop();
    let (writer, reader, ledger) = Buffer::from_config_inner(config, usage_handle.clone())
        .await
        .expect("should not fail to create buffer");
    (writer, reader, ledger, usage_handle)
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

#[tokio::test]
async fn async_file_truncate_rejects_extension() {
    crate::test::with_temp_dir(|dir| {
        let path = dir.join("truncate-test");

        async move {
            let mut file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .read(true)
                .write(true)
                .open(path)
                .await
                .expect("file should open");
            file.write_all(b"test").await.expect("write should succeed");
            file.flush().await.expect("flush should succeed");

            file.truncate(2).await.expect("truncation should succeed");
            assert_eq!(
                file.metadata().await.expect("metadata should load").len(),
                2
            );

            let error = file.truncate(3).await.expect_err("extension should fail");
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
            assert_eq!(
                file.metadata().await.expect("metadata should load").len(),
                2
            );
        }
    })
    .await;
}

#[tokio::test]
async fn production_filesystem_truncates_with_append_handle_open() {
    with_temp_dir(|dir| {
        let path = dir.join("truncate-with-append-handle");

        async move {
            let filesystem = ProductionFilesystem::default();
            filesystem
                .truncate_file(&path, 0)
                .await
                .expect("truncating a missing file to zero should create it");
            let mut append_file = filesystem
                .open_file_writable(&path)
                .await
                .expect("opening append file should succeed");
            append_file
                .write_all(b"abcdef")
                .await
                .expect("initial write should succeed");
            append_file.flush().await.expect("flush should succeed");

            filesystem
                .truncate_file(&path, 2)
                .await
                .expect("truncation should succeed");

            append_file
                .write_all(b"z")
                .await
                .expect("append after truncation should succeed");
            append_file.flush().await.expect("flush should succeed");
            drop(append_file);

            assert_eq!(
                b"abz",
                tokio::fs::read(path)
                    .await
                    .expect("reading truncated file should succeed")
                    .as_slice()
            );
        }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn production_write_completes() {
    with_temp_dir(|dir| {
        let path = dir.join("single-poll-write");

        async move {
            let filesystem = ProductionFilesystem::default();
            let mut file = filesystem
                .open_file_writable(&path)
                .await
                .expect("file should open");

            assert_eq!(
                file.write_resumable(b"test")
                    .await
                    .expect("write should succeed"),
                4
            );
            assert_eq!(
                file.metadata().await.expect("metadata should load").len(),
                4
            );
        }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn cancelled_stalled_write_does_not_block_runtime_and_retains_session_lock() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let super::io::StalledWrites { filesystem, gate } =
                ProductionFilesystem::with_stalled_writes();
            let ledger = open_production_ledger(&data_dir, filesystem.clone())
                .await
                .unwrap();
            let _gate_cleanup = WriteGateCleanup(Arc::clone(&gate));
            let path = data_dir.join("buffer-data-final");
            let mut file = ledger.filesystem().open_file_writable(&path).await.unwrap();
            let mut write = Box::pin(file.write_resumable(b"test"));

            tokio::select! {
                result = &mut write => panic!("gated write unexpectedly completed: {result:?}"),
                () = gate.wait_until_started() => {}
            }

            let runtime_progressed = Arc::new(AtomicBool::new(false));
            let watchdog_progress = Arc::clone(&runtime_progressed);
            let watchdog_gate = Arc::clone(&gate);
            let watchdog = std::thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(5);
                while !watchdog_progress.load(Ordering::Acquire) && Instant::now() < deadline {
                    std::thread::park_timeout(deadline.saturating_duration_since(Instant::now()));
                }
                let runtime_blocked = !watchdog_progress.load(Ordering::Acquire);
                if runtime_blocked {
                    watchdog_gate.release();
                }
                runtime_blocked
            });

            drop(write);
            drop(file);
            drop(ledger);
            tokio::task::yield_now().await;
            runtime_progressed.store(true, Ordering::Release);
            watchdog.thread().unpark();

            assert_session_locked(&data_dir).await;

            gate.release();
            assert!(
                !watchdog.join().unwrap(),
                "the watchdog had to release a write that parked the current-thread runtime"
            );
            wait_for_session_unlock(&data_dir).await;
        }
    })
    .await;
}

#[tokio::test]
async fn readable_production_file_does_not_create_blocking_writer() {
    with_temp_dir(|dir| {
        let path = dir.join("readable-file");

        async move {
            tokio::fs::write(&path, b"test").await.unwrap();
            let filesystem = ProductionFilesystem::default();
            let file = filesystem.open_file_readable(&path).await.unwrap();

            assert!(!file.has_blocking_writer());
        }
    })
    .await;
}

#[tokio::test]
async fn cloned_production_filesystems_keep_session_locks_independent() {
    with_temp_dir(|dir| {
        let first_dir = dir.join("first");
        let second_dir = dir.join("second");

        async move {
            let super::io::StalledWrites { filesystem, gate } =
                ProductionFilesystem::with_stalled_writes();
            let _gate_cleanup = WriteGateCleanup(Arc::clone(&gate));
            let first = open_production_ledger(&first_dir, filesystem.clone())
                .await
                .unwrap();
            let second = open_production_ledger(&second_dir, filesystem)
                .await
                .unwrap();

            let mut first_file = first
                .filesystem()
                .open_file_writable(&first_dir.join("buffer-data-final"))
                .await
                .unwrap();
            let mut write = Box::pin(first_file.write_resumable(b"test"));
            tokio::select! {
                result = &mut write => panic!("gated write unexpectedly completed: {result:?}"),
                () = gate.wait_until_started() => {}
            }
            drop(write);
            drop(first);

            assert_session_locked(&first_dir).await;

            gate.release();
            assert_eq!(first_file.write_resumable(b"test").await.unwrap(), 4);
            drop(first_file);

            reopen_production_ledger(&first_dir)
                .await
                .expect("first session lock should be released independently");

            assert_session_locked(&second_dir).await;
            drop(second);

            reopen_production_ledger(&second_dir)
                .await
                .expect("second session lock should be released independently");
        }
    })
    .await;
}
