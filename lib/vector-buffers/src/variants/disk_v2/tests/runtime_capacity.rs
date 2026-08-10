use std::{
    io,
    time::{Duration, Instant},
};

use tokio::time::{sleep, timeout};

use super::{
    Buffer, DiskBufferConfigBuilder,
    model::{filesystem::TestFilesystem, record::Record},
};
use crate::buffer_usage_data::BufferUsageHandle;
use crate::variants::disk_v2::TryWriteOutcome;

async fn build_buffer(
    filesystem: TestFilesystem,
    write_buffer_size: usize,
) -> (
    super::BufferWriter<Record, TestFilesystem>,
    super::BufferReader<Record, TestFilesystem>,
    std::sync::Arc<super::Ledger<TestFilesystem>>,
    BufferUsageHandle,
) {
    let directory =
        std::env::temp_dir().join(format!("vector-disk-v2-capacity-{}", rand::random::<u64>()));
    let config = DiskBufferConfigBuilder::from_path(directory)
        .write_buffer_size(write_buffer_size)
        .filesystem(filesystem)
        .build()
        .expect("configuration should be valid");
    let usage = BufferUsageHandle::noop();
    let (writer, reader, ledger) = Buffer::from_config_inner(config, usage.clone())
        .await
        .expect("buffer should initialize");
    (writer, reader, ledger, usage)
}

async fn build_rotating_buffer(
    filesystem: TestFilesystem,
) -> (
    super::BufferWriter<Record, TestFilesystem>,
    super::BufferReader<Record, TestFilesystem>,
    std::sync::Arc<super::Ledger<TestFilesystem>>,
) {
    let directory =
        std::env::temp_dir().join(format!("vector-disk-v2-open-{}", rand::random::<u64>()));
    let config = DiskBufferConfigBuilder::from_path(directory)
        .max_data_file_size(256)
        .max_record_size(256)
        .write_buffer_size(1024)
        .filesystem(filesystem)
        .build()
        .expect("configuration should be valid");
    let (writer, reader, ledger) = Buffer::from_config_inner(config, BufferUsageHandle::noop())
        .await
        .expect("buffer should initialize");
    (writer, reader, ledger)
}

async fn assert_flush_pending(writer: &mut super::BufferWriter<Record, TestFilesystem>) {
    assert!(
        timeout(Duration::from_millis(20), writer.flush())
            .await
            .is_err(),
        "flush should remain pending while persistence is capacity-blocked"
    );
}

#[tokio::test]
async fn partial_write_then_storage_full_resumes_without_duplication() {
    let filesystem = TestFilesystem::default();
    let (mut writer, mut reader, ledger, usage) = build_buffer(filesystem.clone(), 16).await;
    let record = Record::new(1, 128, 1);

    filesystem.fail_data_writes_after(7, io::ErrorKind::StorageFull);
    assert_eq!(
        writer.try_write_record(record.clone()).await.unwrap(),
        TryWriteOutcome::Written
    );
    assert_flush_pending(&mut writer).await;
    assert_eq!(ledger.get_total_records(), 0);
    assert_eq!(ledger.get_total_buffer_size(), 0);
    assert_eq!(usage.snapshot().received_event_count, 0);

    let path = ledger.get_current_writer_data_file_path();
    assert_eq!(filesystem.data(&path).len(), 7);

    filesystem.restore_data_writes();
    writer.flush().await.unwrap();
    let written_len = filesystem.data(&path).len();
    assert_eq!(ledger.get_total_records(), 1);
    assert_eq!(ledger.get_total_buffer_size(), written_len as u64);
    assert_eq!(usage.snapshot().received_event_count, 1);
    assert_eq!(reader.next().await.unwrap(), Some(record));
}

#[tokio::test]
async fn large_record_uses_resumable_flush_path() {
    let filesystem = TestFilesystem::default();
    filesystem.set_max_write_size(Some(5));
    let (mut writer, mut reader, ledger, _) = build_buffer(filesystem.clone(), 16).await;
    let record = Record::new(2, 256, 1);

    filesystem.fail_data_writes_after(10, io::ErrorKind::StorageFull);
    assert_eq!(
        writer.try_write_record(record.clone()).await.unwrap(),
        TryWriteOutcome::Written
    );
    assert_flush_pending(&mut writer).await;
    filesystem.restore_data_writes();
    writer.flush().await.unwrap();

    assert_eq!(
        filesystem
            .data(&ledger.get_current_writer_data_file_path())
            .len() as u64,
        ledger.get_total_buffer_size()
    );
    assert_eq!(reader.next().await.unwrap(), Some(record));
}

#[tokio::test]
async fn runtime_block_waits_and_recovers_from_timer() {
    let filesystem = TestFilesystem::default();
    let (mut writer, _, _, _) = build_buffer(filesystem.clone(), 16).await;
    filesystem.fail_data_writes_after(0, io::ErrorKind::StorageFull);
    assert_eq!(
        writer
            .try_write_record(Record::new(3, 64, 1))
            .await
            .unwrap(),
        TryWriteOutcome::Written
    );

    let restore = filesystem.clone();
    tokio::spawn(async move {
        sleep(Duration::from_millis(25)).await;
        restore.restore_data_writes();
    });
    timeout(Duration::from_secs(1), writer.flush())
        .await
        .expect("flush should recover without reader notification")
        .expect("flush should succeed");
}

#[tokio::test]
async fn runtime_block_reacts_to_reader_notification() {
    let filesystem = TestFilesystem::default();
    let (mut writer, _, ledger, _) = build_buffer(filesystem.clone(), 16).await;
    filesystem.fail_data_writes_after(0, io::ErrorKind::StorageFull);
    assert_eq!(
        writer
            .try_write_record(Record::new(4, 64, 1))
            .await
            .unwrap(),
        TryWriteOutcome::Written
    );
    let restore = filesystem.clone();
    let notify = std::sync::Arc::clone(&ledger);
    tokio::spawn(async move {
        sleep(Duration::from_millis(10)).await;
        restore.restore_data_writes();
        notify.notify_reader_waiters();
    });
    timeout(Duration::from_millis(50), writer.flush())
        .await
        .expect("reader progress should wake the writer")
        .expect("flush should succeed");
}

#[tokio::test]
async fn runtime_rotation_open_capacity_waits_without_spinning_and_recovers() {
    let filesystem = TestFilesystem::default();
    let (mut writer, _, _) = build_rotating_buffer(filesystem.clone()).await;
    for id in 20..22 {
        assert_eq!(
            writer
                .try_write_record(Record::new(id, 64, 1))
                .await
                .unwrap(),
            TryWriteOutcome::Written
        );
        writer.flush().await.unwrap();
    }

    filesystem.fail_data_file_open(io::ErrorKind::StorageFull);
    let current = Record::new(22, 64, 1);
    let restore = filesystem.clone();
    tokio::spawn(async move {
        sleep(Duration::from_millis(250)).await;
        restore.restore_data_file_open();
    });
    let started = Instant::now();
    let mut write = Box::pin(writer.try_write_record(current));
    assert!(
        timeout(Duration::from_millis(20), &mut write)
            .await
            .is_err(),
        "runtime open capacity should keep try_write pending"
    );
    assert_eq!(filesystem.data_file_open_attempts(), 1);
    assert_eq!(
        timeout(Duration::from_secs(2), write)
            .await
            .expect("try_write should recover")
            .unwrap(),
        TryWriteOutcome::Written
    );
    assert!(started.elapsed() >= Duration::from_millis(250));
    assert!(filesystem.data_file_open_attempts() <= 3);
}

#[tokio::test]
async fn runtime_rotation_sync_capacity_keeps_try_write_pending() {
    let filesystem = TestFilesystem::default();
    let (mut writer, _, ledger) = build_rotating_buffer(filesystem.clone()).await;
    for id in 30..32 {
        assert_eq!(
            writer
                .try_write_record(Record::new(id, 64, 1))
                .await
                .unwrap(),
            TryWriteOutcome::Written
        );
        writer.flush().await.unwrap();
    }

    let writer_file_id = ledger.get_current_writer_file_id();
    // Let the full current file sync successfully, then fail syncing the newly created file.
    filesystem.fail_data_file_sync_after(1, io::ErrorKind::StorageFull);
    let current = Record::new(32, 64, 1);
    let mut write = Box::pin(writer.try_write_record(current));
    assert!(
        timeout(Duration::from_millis(20), &mut write)
            .await
            .is_err(),
        "runtime sync capacity should keep try_write pending"
    );
    assert_eq!(ledger.get_current_writer_file_id(), writer_file_id);

    filesystem.restore_data_file_sync();
    assert_eq!(
        timeout(Duration::from_secs(1), write)
            .await
            .expect("try_write should recover")
            .unwrap(),
        TryWriteOutcome::Written
    );
    assert_eq!(ledger.get_current_writer_file_id(), writer_file_id + 1);
}

#[tokio::test]
async fn try_write_waits_while_previous_accepted_data_retries() {
    let filesystem = TestFilesystem::default();
    let (mut writer, mut reader, _, _) = build_buffer(filesystem.clone(), 16).await;
    let accepted = Record::new(5, 64, 1);
    let subsequent = Record::new(6, 64, 1);
    filesystem.fail_data_writes_after(3, io::ErrorKind::StorageFull);
    assert_eq!(
        writer.try_write_record(accepted.clone()).await.unwrap(),
        TryWriteOutcome::Written
    );
    let mut write = Box::pin(writer.try_write_record(subsequent.clone()));
    assert!(
        timeout(Duration::from_millis(20), &mut write)
            .await
            .is_err(),
        "try_write should wait for pending accepted data"
    );
    filesystem.restore_data_writes();
    assert_eq!(
        timeout(Duration::from_secs(1), write)
            .await
            .expect("try_write should recover")
            .unwrap(),
        TryWriteOutcome::Written
    );
    writer.flush().await.unwrap();
    assert_eq!(reader.next().await.unwrap(), Some(accepted));
    assert_eq!(reader.next().await.unwrap(), Some(subsequent));
}

#[tokio::test]
async fn implicit_flush_capacity_error_keeps_current_write_pending() {
    let filesystem = TestFilesystem::default();
    let (mut writer, mut reader, _, _) = build_buffer(filesystem.clone(), 128).await;
    let accepted = Record::new(12, 32, 1);
    let current = Record::new(13, 32, 1);
    assert_eq!(
        writer.try_write_record(accepted.clone()).await.unwrap(),
        TryWriteOutcome::Written
    );

    filesystem.fail_data_writes_after(4, io::ErrorKind::StorageFull);
    let mut write = Box::pin(writer.try_write_record(current.clone()));
    assert!(
        timeout(Duration::from_millis(20), &mut write)
            .await
            .is_err(),
        "implicit flush capacity should keep try_write pending"
    );

    filesystem.restore_data_writes();
    assert_eq!(
        timeout(Duration::from_secs(1), write)
            .await
            .expect("try_write should recover")
            .unwrap(),
        TryWriteOutcome::Written
    );
    writer.flush().await.unwrap();
    assert_eq!(reader.next().await.unwrap(), Some(accepted));
    assert_eq!(reader.next().await.unwrap(), Some(current));
}

#[tokio::test]
async fn non_capacity_runtime_io_error_remains_fatal() {
    let filesystem = TestFilesystem::default();
    let (mut writer, _, _, _) = build_buffer(filesystem.clone(), 1024).await;
    assert_eq!(
        writer
            .try_write_record(Record::new(7, 64, 1))
            .await
            .unwrap(),
        TryWriteOutcome::Written
    );
    filesystem.fail_data_writes_after(0, io::ErrorKind::PermissionDenied);

    let error = writer
        .flush()
        .await
        .expect_err("permission failure should be fatal");
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
}

#[tokio::test]
async fn startup_storage_full_is_immediately_fatal() {
    let filesystem = TestFilesystem::default();
    filesystem.fail_data_file_open(io::ErrorKind::StorageFull);
    let directory = std::env::temp_dir().join(format!(
        "vector-disk-v2-capacity-startup-{}",
        rand::random::<u64>()
    ));
    let config = DiskBufferConfigBuilder::from_path(directory)
        .filesystem(filesystem)
        .build()
        .unwrap();

    let result = Buffer::<Record>::from_config_inner(config, BufferUsageHandle::noop()).await;
    assert!(result.is_err());
}
