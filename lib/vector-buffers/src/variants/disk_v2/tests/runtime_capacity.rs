use std::{io, time::Duration};

use tokio::time::timeout;
use vector_common::finalization::{AddBatchNotifier, BatchNotifier, BatchStatus};

use super::{
    Buffer, DiskBufferConfigBuilder,
    model::{filesystem::TestFilesystem, record::Record},
};
use crate::buffer_usage_data::BufferUsageHandle;
use crate::variants::disk_v2::{CapacityProgress, TryWriteOutcome, WriterError};

struct CapacityBuffer {
    writer: super::BufferWriter<Record, TestFilesystem>,
    reader: super::BufferReader<Record, TestFilesystem>,
    ledger: std::sync::Arc<super::Ledger<TestFilesystem>>,
    usage: BufferUsageHandle,
}

async fn build_buffer_with_rotation(
    filesystem: TestFilesystem,
    write_buffer_size: usize,
    rotating: bool,
) -> CapacityBuffer {
    let directory =
        std::env::temp_dir().join(format!("vector-disk-v2-capacity-{}", rand::random::<u64>()));
    let mut builder = DiskBufferConfigBuilder::from_path(directory)
        .write_buffer_size(write_buffer_size)
        .filesystem(filesystem);
    if rotating {
        builder = builder.max_data_file_size(256).max_record_size(256);
    }
    let config = builder.build().expect("configuration should be valid");
    let usage = BufferUsageHandle::noop();
    let (writer, reader, ledger) = Buffer::from_config_inner(config, usage.clone())
        .await
        .expect("buffer should initialize");
    CapacityBuffer {
        writer,
        reader,
        ledger,
        usage,
    }
}

async fn build_buffer(filesystem: TestFilesystem, write_buffer_size: usize) -> CapacityBuffer {
    build_buffer_with_rotation(filesystem, write_buffer_size, false).await
}

async fn build_rotating_buffer(filesystem: TestFilesystem) -> CapacityBuffer {
    build_buffer_with_rotation(filesystem, 1024, true).await
}

async fn assert_flush_pending(writer: &mut super::BufferWriter<Record, TestFilesystem>) {
    assert!(
        timeout(Duration::from_millis(20), writer.flush())
            .await
            .is_err(),
        "flush should remain pending while persistence is capacity-blocked"
    );
}

async fn assert_written(writer: &mut super::BufferWriter<Record, TestFilesystem>, record: Record) {
    assert_eq!(
        writer.try_write_record(record).await.unwrap(),
        TryWriteOutcome::Written
    );
}

async fn cancel_capacity_write(
    writer: &mut super::BufferWriter<Record, TestFilesystem>,
    filesystem: &TestFilesystem,
    record: Record,
    bytes_until_error: usize,
) {
    filesystem.fail_data_writes_after(bytes_until_error, io::ErrorKind::StorageFull);
    assert_eq!(
        writer.try_write_record(record).await.unwrap(),
        TryWriteOutcome::Pending,
        "a partially written record must remain owned by the writer"
    );
    filesystem.restore_data_writes();
}

async fn fill_for_rotation(
    writer: &mut super::BufferWriter<Record, TestFilesystem>,
    first_id: u32,
) {
    for id in first_id..first_id + 2 {
        assert_written(writer, Record::new(id, 64, 1)).await;
        writer.flush().await.unwrap();
    }
}

#[derive(Clone, Copy, Debug)]
enum OpenPath {
    Atomic,
    Fallback,
}

impl OpenPath {
    fn prepare(self, filesystem: &TestFilesystem, ledger: &super::Ledger<TestFilesystem>) {
        if matches!(self, Self::Fallback) {
            filesystem.create_data_file(&ledger.get_next_writer_data_file_path());
        }
    }

    fn fail(self, filesystem: &TestFilesystem, kind: io::ErrorKind) {
        match self {
            Self::Atomic => filesystem.fail_data_file_open(kind),
            Self::Fallback => filesystem.fail_data_file_fallback_open(kind),
        }
    }

    fn restore(self, filesystem: &TestFilesystem) {
        match self {
            Self::Atomic => filesystem.restore_data_file_open(),
            Self::Fallback => filesystem.restore_data_file_fallback_open(),
        }
    }

    fn attempts(self, filesystem: &TestFilesystem) -> usize {
        match self {
            Self::Atomic => filesystem.data_file_open_attempts(),
            Self::Fallback => filesystem.data_file_fallback_open_attempts(),
        }
    }
}

#[tokio::test]
async fn partial_write_then_storage_full_resumes_without_duplication() {
    let filesystem = TestFilesystem::default();
    let CapacityBuffer {
        mut writer,
        mut reader,
        ledger,
        usage,
    } = build_buffer(filesystem.clone(), 1024).await;
    let record = Record::new(1, 128, 1);

    filesystem.fail_data_writes_after(7, io::ErrorKind::StorageFull);
    assert_written(&mut writer, record.clone()).await;
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
async fn partial_write_leaves_subsequent_record_available_for_drop_newest() {
    let filesystem = TestFilesystem::default();
    filesystem.set_max_write_size(Some(5));
    let CapacityBuffer {
        mut writer,
        mut reader,
        ..
    } = build_buffer(filesystem.clone(), 16).await;
    let owned = Record::new(60, 256, 1);
    let newest = Record::new(61, 64, 1);

    filesystem.fail_data_writes_after(10, io::ErrorKind::StorageFull);
    assert_eq!(
        writer.try_write_record(owned.clone()).await.unwrap(),
        TryWriteOutcome::Pending
    );
    assert_eq!(
        timeout(
            Duration::from_millis(20),
            writer.try_write_record(newest.clone())
        )
        .await
        .expect("a subsequent nonblocking send must return promptly")
        .unwrap(),
        TryWriteOutcome::Full(newest)
    );

    filesystem.restore_data_writes();
    writer.flush().await.unwrap();
    assert_eq!(reader.next().await.unwrap(), Some(owned));
}

#[tokio::test]
async fn partial_write_leaves_subsequent_record_available_for_overflow() {
    let filesystem = TestFilesystem::default();
    filesystem.set_max_write_size(Some(5));
    let CapacityBuffer {
        mut writer,
        mut reader,
        ..
    } = build_buffer(filesystem.clone(), 16).await;
    let CapacityBuffer {
        writer: mut overflow_writer,
        reader: mut overflow_reader,
        ..
    } = build_buffer(TestFilesystem::default(), 1024).await;
    let owned = Record::new(62, 256, 1);
    let overflowed = Record::new(63, 64, 1);

    filesystem.fail_data_writes_after(10, io::ErrorKind::StorageFull);
    assert_eq!(
        writer.try_write_record(owned.clone()).await.unwrap(),
        TryWriteOutcome::Pending
    );
    let TryWriteOutcome::Full(returned) =
        writer.try_write_record(overflowed.clone()).await.unwrap()
    else {
        panic!("the subsequent record must remain available for overflow");
    };
    assert_written(&mut overflow_writer, returned).await;

    filesystem.restore_data_writes();
    writer.flush().await.unwrap();
    overflow_writer.flush().await.unwrap();
    assert_eq!(reader.next().await.unwrap(), Some(owned));
    assert_eq!(overflow_reader.next().await.unwrap(), Some(overflowed));
}

#[tokio::test]
async fn partial_write_retains_finalizer_until_owned_record_completes() {
    let filesystem = TestFilesystem::default();
    filesystem.set_max_write_size(Some(5));
    let CapacityBuffer { mut writer, .. } = build_buffer(filesystem.clone(), 16).await;
    let mut record = Record::new(64, 256, 1);
    let (batch, mut finalizer) = BatchNotifier::new_with_receiver();
    record.add_batch_notifier(batch);

    filesystem.fail_data_writes_after(10, io::ErrorKind::StorageFull);
    assert_eq!(
        writer.try_write_record(record).await.unwrap(),
        TryWriteOutcome::Pending
    );
    assert!(
        timeout(Duration::from_millis(20), &mut finalizer)
            .await
            .is_err(),
        "an owned record must retain its finalizer while capacity is exhausted"
    );

    filesystem.restore_data_writes();
    writer.flush().await.unwrap();
    assert_eq!(finalizer.await, BatchStatus::Delivered);
}

#[tokio::test]
async fn unowned_capacity_full_record_keeps_finalizer_for_policy() {
    let filesystem = TestFilesystem::default();
    let CapacityBuffer { mut writer, .. } = build_rotating_buffer(filesystem.clone()).await;
    fill_for_rotation(&mut writer, 70).await;
    filesystem.fail_data_file_open(io::ErrorKind::StorageFull);
    let mut record = Record::new(72, 64, 1);
    let (batch, mut finalizer) = BatchNotifier::new_with_receiver();
    record.add_batch_notifier(batch);

    let TryWriteOutcome::Full(returned) = writer.try_write_record(record).await.unwrap() else {
        panic!("open ENOSPC must return the unowned record to policy");
    };
    assert!(
        timeout(Duration::from_millis(20), &mut finalizer)
            .await
            .is_err(),
        "returning an unowned record must not resolve its finalizer"
    );
    drop(returned);
    assert_eq!(finalizer.await, BatchStatus::Delivered);
}

#[tokio::test]
async fn large_record_uses_resumable_flush_path() {
    let filesystem = TestFilesystem::default();
    filesystem.set_max_write_size(Some(5));
    let CapacityBuffer {
        mut writer,
        mut reader,
        ledger,
        ..
    } = build_buffer(filesystem.clone(), 16).await;
    let record = Record::new(2, 256, 1);

    filesystem.fail_data_writes_after(10, io::ErrorKind::StorageFull);
    assert_eq!(
        writer.try_write_record(record.clone()).await.unwrap(),
        TryWriteOutcome::Pending
    );
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
async fn cancelled_large_write_finishes_before_next_record() {
    let filesystem = TestFilesystem::default();
    filesystem.set_max_write_size(Some(5));
    let CapacityBuffer {
        mut writer,
        mut reader,
        ..
    } = build_buffer(filesystem.clone(), 16).await;
    let interrupted = Record::new(8, 256, 1);
    let subsequent = Record::new(9, 256, 1);

    cancel_capacity_write(&mut writer, &filesystem, interrupted.clone(), 10).await;
    assert_written(&mut writer, subsequent.clone()).await;
    writer.flush().await.unwrap();

    assert_eq!(reader.next().await.unwrap(), Some(interrupted));
    assert_eq!(reader.next().await.unwrap(), Some(subsequent));
}

#[tokio::test]
async fn flush_finishes_cancelled_large_write() {
    let filesystem = TestFilesystem::default();
    let CapacityBuffer {
        mut writer,
        mut reader,
        ..
    } = build_buffer(filesystem.clone(), 16).await;
    let interrupted = Record::new(13, 256, 1);

    cancel_capacity_write(&mut writer, &filesystem, interrupted.clone(), 10).await;
    writer.flush().await.unwrap();

    assert_eq!(reader.next().await.unwrap(), Some(interrupted));
}

#[tokio::test]
async fn cancelled_large_write_preserves_implicit_flush_accounting() {
    let filesystem = TestFilesystem::default();
    let CapacityBuffer {
        mut writer,
        mut reader,
        ledger,
        ..
    } = build_buffer(filesystem.clone(), 128).await;
    let buffered = Record::new(10, 16, 1);
    let interrupted = Record::new(11, 256, 1);
    let subsequent = Record::new(12, 16, 1);

    assert_written(&mut writer, buffered.clone()).await;
    cancel_capacity_write(&mut writer, &filesystem, interrupted.clone(), 200).await;

    assert_eq!(ledger.get_total_records(), 1);
    assert_eq!(reader.next().await.unwrap(), Some(buffered));

    assert_written(&mut writer, subsequent.clone()).await;
    writer.flush().await.unwrap();

    assert_eq!(ledger.get_total_records(), 3);
    assert_eq!(reader.next().await.unwrap(), Some(interrupted));
    assert_eq!(reader.next().await.unwrap(), Some(subsequent));
}

#[tokio::test(start_paused = true)]
async fn runtime_block_waits_and_recovers_from_timer() {
    vector_common::event_test_util::clear_recorded_events();
    let filesystem = TestFilesystem::default();
    let CapacityBuffer { mut writer, .. } = build_buffer(filesystem.clone(), 1024).await;
    filesystem.fail_data_writes_after(0, io::ErrorKind::StorageFull);
    assert_written(&mut writer, Record::new(3, 64, 1)).await;

    let mut flush = Box::pin(writer.flush());
    tokio::select! {
        result = &mut flush => panic!("flush unexpectedly completed: {result:?}"),
        () = tokio::task::yield_now() => {}
    }
    assert_eq!(filesystem.data_write_attempts(), 1);
    vector_common::event_test_util::contains_name_once("DiskBufferBackpressure").unwrap();
    assert!(
        vector_common::event_test_util::contains_name_once("DiskBufferBackpressureRecovered")
            .is_err()
    );

    tokio::time::advance(Duration::from_millis(100)).await;
    tokio::select! {
        result = &mut flush => panic!("flush unexpectedly completed: {result:?}"),
        () = tokio::task::yield_now() => {}
    }
    assert_eq!(filesystem.data_write_attempts(), 2);
    filesystem.restore_data_writes();
    tokio::time::advance(Duration::from_millis(200)).await;
    timeout(Duration::from_secs(1), flush)
        .await
        .expect("flush should recover without reader notification")
        .expect("flush should succeed");
    vector_common::event_test_util::contains_name_once("DiskBufferBackpressureRecovered").unwrap();
}

#[tokio::test(start_paused = true)]
async fn cancelled_flush_backpressures_small_subsequent_write() {
    vector_common::event_test_util::clear_recorded_events();
    let filesystem = TestFilesystem::default();
    let CapacityBuffer {
        mut writer,
        mut reader,
        ledger,
        ..
    } = build_buffer(filesystem.clone(), 1024).await;
    let accepted = Record::new(15, 32, 1);
    let subsequent = Record::new(16, 32, 1);
    assert_written(&mut writer, accepted.clone()).await;

    filesystem.fail_data_writes_after(0, io::ErrorKind::StorageFull);
    let mut flush = Box::pin(writer.flush());
    tokio::select! {
        result = &mut flush => panic!("flush unexpectedly completed: {result:?}"),
        () = tokio::task::yield_now() => {}
    }
    drop(flush);
    assert_eq!(filesystem.data_write_attempts(), 1);

    assert_eq!(
        writer.try_write_record(subsequent.clone()).await.unwrap(),
        TryWriteOutcome::Full(subsequent.clone())
    );
    assert_eq!(filesystem.data_write_attempts(), 2);
    vector_common::event_test_util::contains_name_once("DiskBufferBackpressure").unwrap();
    assert!(
        vector_common::event_test_util::contains_name_once("DiskBufferBackpressureRecovered")
            .is_err()
    );

    filesystem.restore_data_writes();
    ledger.notify_reader_waiters();
    assert_written(&mut writer, subsequent.clone()).await;
    vector_common::event_test_util::contains_name_once("DiskBufferBackpressureRecovered").unwrap();

    writer.flush().await.unwrap();
    assert_eq!(reader.next().await.unwrap(), Some(accepted));
    assert_eq!(reader.next().await.unwrap(), Some(subsequent));
}

#[tokio::test(start_paused = true)]
async fn runtime_block_reacts_to_reader_notification() {
    let filesystem = TestFilesystem::default();
    let CapacityBuffer {
        mut writer, ledger, ..
    } = build_buffer(filesystem.clone(), 1024).await;
    filesystem.fail_data_writes_after(0, io::ErrorKind::StorageFull);
    assert_written(&mut writer, Record::new(4, 64, 1)).await;
    let mut flush = Box::pin(writer.flush());
    tokio::select! {
        result = &mut flush => panic!("flush unexpectedly completed: {result:?}"),
        () = tokio::task::yield_now() => {}
    }
    filesystem.restore_data_writes();
    ledger.notify_reader_waiters();
    timeout(Duration::from_secs(1), flush)
        .await
        .expect("reader progress should wake the writer")
        .expect("flush should succeed");
}

#[tokio::test(start_paused = true)]
async fn runtime_rotation_capacity_faults_wait_without_spinning_and_recover() {
    for path in [OpenPath::Atomic, OpenPath::Fallback] {
        vector_common::event_test_util::clear_recorded_events();
        let filesystem = TestFilesystem::default();
        let CapacityBuffer {
            mut writer, ledger, ..
        } = build_rotating_buffer(filesystem.clone()).await;
        fill_for_rotation(&mut writer, 20).await;
        path.prepare(&filesystem, &ledger);
        path.fail(&filesystem, io::ErrorKind::StorageFull);

        let current = Record::new(22, 64, 1);
        let outcome = writer.try_write_record(current.clone()).await.unwrap();
        assert_eq!(outcome, TryWriteOutcome::Full(current.clone()));
        assert_eq!(path.attempts(&filesystem), 1);
        assert_eq!(
            writer.retry_capacity().await.unwrap(),
            CapacityProgress::Blocked
        );
        assert_eq!(path.attempts(&filesystem), 2);

        path.restore(&filesystem);
        assert_eq!(
            writer.retry_capacity().await.unwrap(),
            CapacityProgress::Ready
        );
        assert_written(&mut writer, current).await;
        assert!(path.attempts(&filesystem) <= 3);
        vector_common::event_test_util::contains_name_once("DiskBufferBackpressure").unwrap();
        vector_common::event_test_util::contains_name_once("DiskBufferBackpressureRecovered")
            .unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn successful_flush_does_not_close_cancelled_open_capacity_episode() {
    vector_common::event_test_util::clear_recorded_events();
    let filesystem = TestFilesystem::default();
    let CapacityBuffer {
        mut writer, ledger, ..
    } = build_rotating_buffer(filesystem.clone()).await;
    fill_for_rotation(&mut writer, 40).await;
    let path = OpenPath::Atomic;
    path.prepare(&filesystem, &ledger);
    path.fail(&filesystem, io::ErrorKind::StorageFull);

    assert!(matches!(
        writer
            .try_write_record(Record::new(42, 64, 1))
            .await
            .unwrap(),
        TryWriteOutcome::Full(_)
    ));
    writer.flush().await.unwrap();
    assert!(
        vector_common::event_test_util::contains_name_once("DiskBufferBackpressureRecovered")
            .is_err(),
        "a successful data flush must not close an open-file capacity episode"
    );

    path.restore(&filesystem);
    assert_written(&mut writer, Record::new(43, 64, 1)).await;
    vector_common::event_test_util::contains_name_once("DiskBufferBackpressureRecovered").unwrap();
}

#[tokio::test]
async fn runtime_rotation_non_capacity_open_faults_remain_fatal() {
    for path in [OpenPath::Atomic, OpenPath::Fallback] {
        vector_common::event_test_util::clear_recorded_events();
        let filesystem = TestFilesystem::default();
        let CapacityBuffer {
            mut writer, ledger, ..
        } = build_rotating_buffer(filesystem.clone()).await;
        fill_for_rotation(&mut writer, 50).await;
        path.prepare(&filesystem, &ledger);
        path.fail(&filesystem, io::ErrorKind::PermissionDenied);

        let error = writer
            .try_write_record(Record::new(52, 64, 1))
            .await
            .expect_err("permission failure must remain fatal");
        let WriterError::Io { source } = error else {
            panic!("expected I/O error, got {error:?}");
        };
        assert_eq!(source.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(path.attempts(&filesystem), 1);
        assert!(
            vector_common::event_test_util::contains_name_once("DiskBufferBackpressure").is_err()
        );
    }
}

#[tokio::test]
async fn runtime_rotation_sync_capacity_retries_without_losing_current_record() {
    let filesystem = TestFilesystem::default();
    let CapacityBuffer {
        mut writer, ledger, ..
    } = build_rotating_buffer(filesystem.clone()).await;
    fill_for_rotation(&mut writer, 30).await;

    let writer_file_id = ledger.get_current_writer_file_id();
    // Let the full current file sync successfully, then fail syncing the newly created file.
    #[cfg(unix)]
    filesystem.fail_data_file_sync_after_raw_os_error(1, libc::ENOSPC);
    #[cfg(not(unix))]
    filesystem.fail_data_file_sync_after(1, io::ErrorKind::StorageFull);
    let current = Record::new(32, 64, 1);
    let outcome = writer.try_write_record(current.clone()).await.unwrap();
    assert_eq!(outcome, TryWriteOutcome::Full(current.clone()));
    assert_eq!(ledger.get_current_writer_file_id(), writer_file_id);
    filesystem.restore_data_file_sync();
    assert_eq!(
        writer.retry_capacity().await.unwrap(),
        CapacityProgress::Ready
    );
    assert_written(&mut writer, current).await;
}

#[tokio::test(start_paused = true)]
async fn rotation_flush_capacity_error_is_recorded_once_with_scheduled_retries() {
    vector_common::event_test_util::clear_recorded_events();
    let filesystem = TestFilesystem::default();
    let CapacityBuffer { mut writer, .. } = build_rotating_buffer(filesystem.clone()).await;
    fill_for_rotation(&mut writer, 80).await;
    assert_written(&mut writer, Record::new(82, 64, 1)).await;
    assert_written(&mut writer, Record::new(83, 64, 1)).await;
    let write_attempts_before_fault = filesystem.data_write_attempts();

    #[cfg(unix)]
    filesystem.fail_data_writes_after_raw_os_error(0, libc::ENOSPC);
    #[cfg(not(unix))]
    filesystem.fail_data_writes_after(0, io::ErrorKind::StorageFull);
    let record = Record::new(84, 64, 1);
    assert_eq!(
        writer.try_write_record(record.clone()).await.unwrap(),
        TryWriteOutcome::Full(record.clone())
    );
    assert_eq!(
        filesystem.data_write_attempts() - write_attempts_before_fault,
        1
    );
    #[cfg(unix)]
    assert_eq!(
        writer.capacity_backpressure_details(),
        Some((
            "persist data",
            1,
            io::ErrorKind::StorageFull,
            Some(libc::ENOSPC)
        ))
    );
    #[cfg(not(unix))]
    assert_eq!(
        writer.capacity_backpressure_details(),
        Some(("persist data", 1, io::ErrorKind::StorageFull, None))
    );
    vector_common::event_test_util::contains_name_once("DiskBufferBackpressure").unwrap();

    let mut write = Box::pin(writer.write_record(record.clone()));
    tokio::select! {
        biased;
        result = &mut write => panic!("capacity-blocked rotation unexpectedly completed: {result:?}"),
        () = tokio::task::yield_now() => {}
    }
    assert_eq!(
        filesystem.data_write_attempts() - write_attempts_before_fault,
        2
    );

    tokio::time::advance(Duration::from_millis(100)).await;
    tokio::select! {
        biased;
        result = &mut write => panic!("capacity-blocked rotation unexpectedly completed: {result:?}"),
        () = tokio::task::yield_now() => {}
    }
    assert_eq!(
        filesystem.data_write_attempts() - write_attempts_before_fault,
        3
    );

    tokio::time::advance(Duration::from_millis(100)).await;
    tokio::select! {
        biased;
        result = &mut write => panic!("capacity-blocked rotation unexpectedly completed: {result:?}"),
        () = tokio::task::yield_now() => {}
    }
    assert_eq!(
        filesystem.data_write_attempts() - write_attempts_before_fault,
        3
    );

    filesystem.restore_data_writes();
    tokio::time::advance(Duration::from_millis(100)).await;
    write.await.unwrap();
    vector_common::event_test_util::contains_name_once("DiskBufferBackpressureRecovered").unwrap();
}

#[tokio::test(start_paused = true)]
async fn write_record_capacity_retry_preserves_exponential_backoff() {
    let filesystem = TestFilesystem::default();
    let CapacityBuffer { mut writer, .. } = build_buffer(filesystem.clone(), 16).await;
    filesystem.fail_data_writes_after(0, io::ErrorKind::StorageFull);

    let mut write = Box::pin(writer.write_record(Record::new(85, 64, 1)));
    tokio::select! {
        biased;
        result = &mut write => panic!("capacity-blocked write unexpectedly completed: {result:?}"),
        () = tokio::task::yield_now() => {}
    }
    assert_eq!(filesystem.data_write_attempts(), 1);

    tokio::time::advance(Duration::from_millis(100)).await;
    tokio::select! {
        biased;
        result = &mut write => panic!("capacity-blocked write unexpectedly completed: {result:?}"),
        () = tokio::task::yield_now() => {}
    }
    assert_eq!(filesystem.data_write_attempts(), 2);

    tokio::time::advance(Duration::from_millis(100)).await;
    tokio::select! {
        biased;
        result = &mut write => panic!("capacity-blocked write unexpectedly completed: {result:?}"),
        () = tokio::task::yield_now() => {}
    }
    assert_eq!(filesystem.data_write_attempts(), 2);

    filesystem.restore_data_writes();
    tokio::time::advance(Duration::from_millis(100)).await;
    write.await.unwrap();
}

#[tokio::test]
async fn try_write_waits_while_previous_accepted_data_retries() {
    let filesystem = TestFilesystem::default();
    let CapacityBuffer {
        mut writer,
        mut reader,
        ..
    } = build_buffer(filesystem.clone(), 128).await;
    let accepted = Record::new(5, 64, 1);
    let subsequent = Record::new(6, 64, 1);
    filesystem.fail_data_writes_after(3, io::ErrorKind::StorageFull);
    assert_written(&mut writer, accepted.clone()).await;
    assert_eq!(
        writer.try_write_record(subsequent.clone()).await.unwrap(),
        TryWriteOutcome::Full(subsequent.clone())
    );
    filesystem.restore_data_writes();
    assert_written(&mut writer, subsequent.clone()).await;
    writer.flush().await.unwrap();
    assert_eq!(reader.next().await.unwrap(), Some(accepted));
    assert_eq!(reader.next().await.unwrap(), Some(subsequent));
}

#[tokio::test]
async fn implicit_flush_capacity_error_keeps_current_write_pending() {
    let filesystem = TestFilesystem::default();
    let CapacityBuffer {
        mut writer,
        mut reader,
        ..
    } = build_buffer(filesystem.clone(), 128).await;
    let accepted = Record::new(12, 32, 1);
    let current = Record::new(13, 32, 1);
    assert_written(&mut writer, accepted.clone()).await;

    filesystem.fail_data_writes_after(4, io::ErrorKind::StorageFull);
    assert_eq!(
        writer.try_write_record(current.clone()).await.unwrap(),
        TryWriteOutcome::Full(current.clone()),
        "the current record is still unowned when only the previous buffer flush failed"
    );

    filesystem.restore_data_writes();
    assert_written(&mut writer, current.clone()).await;
    writer.flush().await.unwrap();
    assert_eq!(reader.next().await.unwrap(), Some(accepted));
    assert_eq!(reader.next().await.unwrap(), Some(current));
}

#[tokio::test]
async fn non_capacity_runtime_io_error_remains_fatal() {
    let filesystem = TestFilesystem::default();
    let CapacityBuffer { mut writer, .. } = build_buffer(filesystem.clone(), 1024).await;
    assert_written(&mut writer, Record::new(7, 64, 1)).await;
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
