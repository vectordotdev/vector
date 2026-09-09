use std::{io, sync::Arc, time::Duration};

use temp_dir::TempDir;
use tokio::time::{advance, sleep, timeout};
use vector_common::finalization::{AddBatchNotifier, BatchNotifier, BatchStatus};

use crate::{
    WhenFull,
    buffer_usage_data::BufferUsageHandle,
    topology::channel::{BufferSender, CapacityBlockedHook, SenderAdapter},
    variants::disk_v2::{
        Buffer, DiskBufferConfigBuilder, ProductionFilesystem, StalledWrites, TestWriteGate,
        tests::model::{filesystem::TestFilesystem, record::Record},
    },
};

struct WriteGateCleanup(Arc<TestWriteGate>);

impl Drop for WriteGateCleanup {
    fn drop(&mut self) {
        self.0.release();
    }
}

fn assert_terminal_permission_denied(error: vector_common::Error) {
    let error = error
        .downcast::<io::Error>()
        .expect("terminal error should retain its I/O classification");
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    #[cfg(unix)]
    assert_eq!(error.raw_os_error(), Some(libc::EACCES));
}

fn assert_writer_permission_denied(error: vector_common::Error) {
    let error = error
        .downcast::<crate::variants::disk_v2::WriterError<Record>>()
        .expect("immediate writer error should retain its original type");
    let crate::variants::disk_v2::WriterError::Io { source } = *error else {
        panic!("expected an I/O writer error");
    };
    assert_eq!(source.kind(), io::ErrorKind::PermissionDenied);
}

// Physical disk exhaustion acts like a full buffer for records not yet owned by disk_v2. These
// tests verify each policy while partially written records recover exactly once.
async fn build_policy_disk(
    filesystem: TestFilesystem,
    write_buffer_size: usize,
    rotating: bool,
    when_full: WhenFull,
) -> (
    BufferSender<Record>,
    crate::variants::disk_v2::BufferReader<Record, TestFilesystem>,
    BufferUsageHandle,
) {
    let directory =
        std::env::temp_dir().join(format!("vector-policy-disk-{}", rand::random::<u64>()));
    let mut builder = DiskBufferConfigBuilder::from_path(directory)
        .write_buffer_size(write_buffer_size)
        .filesystem(filesystem);
    if rotating {
        builder = builder.max_data_file_size(256).max_record_size(256);
    }
    let usage = BufferUsageHandle::noop();
    let (writer, reader, _) = Buffer::from_config_inner(builder.build().unwrap(), usage.clone())
        .await
        .unwrap();
    (
        BufferSender::new(SenderAdapter::from(writer), when_full),
        reader,
        usage,
    )
}

async fn build_capacity_blocked_policy_disk(
    when_full: WhenFull,
) -> (
    TestFilesystem,
    BufferSender<Record>,
    crate::variants::disk_v2::BufferReader<Record, TestFilesystem>,
    BufferUsageHandle,
) {
    let filesystem = TestFilesystem::default();
    filesystem.set_max_write_size(Some(5));
    let (sender, reader, usage) = build_policy_disk(filesystem.clone(), 16, false, when_full).await;
    (filesystem, sender, reader, usage)
}

async fn wait_for_received_events(
    usage: &BufferUsageHandle,
    event_count: u64,
    within: Duration,
    message: &'static str,
) {
    timeout(within, async {
        while usage.snapshot().received_event_count < event_count {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect(message);
}

#[tokio::test]
async fn disk_capacity_block_retries_owned_record() {
    let (filesystem, mut sender, mut reader, _) =
        build_capacity_blocked_policy_disk(WhenFull::Block).await;
    let record = Record::new(100, 256, 1);
    filesystem.fail_data_writes_after(10, io::ErrorKind::StorageFull);

    let mut send = Box::pin(sender.send(record.clone(), None));
    assert!(
        timeout(Duration::from_millis(20), &mut send).await.is_err(),
        "block policy must backpressure while the owned record retries"
    );
    filesystem.restore_data_writes();
    timeout(Duration::from_secs(2), send)
        .await
        .expect("block send should recover")
        .unwrap();
    sender.flush().await.unwrap();
    assert_eq!(reader.next().await.unwrap(), Some(record));
}

#[tokio::test]
async fn disk_capacity_drop_newest_is_prompt_and_counted_once() {
    let (filesystem, mut sender, mut reader, usage) =
        build_capacity_blocked_policy_disk(WhenFull::DropNewest).await;
    let outer_usage = BufferUsageHandle::noop();
    sender.with_usage_instrumentation(outer_usage.clone());
    let owned = Record::new(101, 256, 1);
    let dropped = Record::new(102, 64, 1);
    filesystem.fail_data_writes_after(10, io::ErrorKind::StorageFull);

    timeout(Duration::from_secs(1), sender.send(owned.clone(), None))
        .await
        .expect("the owned drop-newest record should be accepted")
        .unwrap();
    timeout(Duration::from_millis(20), sender.send(dropped, None))
        .await
        .expect("drop-newest send must not wait for the retry driver")
        .unwrap();
    assert_eq!(usage.snapshot().dropped_event_count_intentional, 1);
    assert_eq!(outer_usage.snapshot().dropped_event_count_intentional, 0);

    filesystem.restore_data_writes();
    wait_for_received_events(
        &usage,
        2,
        Duration::from_secs(2),
        "owned record should complete in the background",
    )
    .await;
    assert_eq!(reader.next().await.unwrap(), Some(owned));
    let snapshot = usage.snapshot();
    assert_eq!(snapshot.received_event_count, 2);
    assert_eq!(snapshot.dropped_event_count_intentional, 1);
}

#[tokio::test(start_paused = true)]
async fn disk_capacity_drop_newest_flush_skips_writer_while_retrying() {
    let (filesystem, mut sender, _reader, _) =
        build_capacity_blocked_policy_disk(WhenFull::DropNewest).await;
    filesystem.fail_data_writes_after(10, io::ErrorKind::StorageFull);

    sender.send(Record::new(120, 256, 1), None).await.unwrap();
    tokio::task::yield_now().await;
    let write_attempts = filesystem.data_write_attempts();

    for id in 121..124 {
        sender.send(Record::new(id, 64, 1), None).await.unwrap();
        sender.flush().await.unwrap();
    }
    assert_eq!(filesystem.data_write_attempts(), write_attempts);

    // The retry driver is the only path allowed to make another filesystem attempt, and it
    // retains its exponential-backoff schedule while sends and flushes are dropped.
    advance(Duration::from_millis(99)).await;
    tokio::task::yield_now().await;
    assert_eq!(filesystem.data_write_attempts(), write_attempts);
    advance(Duration::from_millis(1)).await;
    tokio::task::yield_now().await;
    assert_eq!(filesystem.data_write_attempts(), write_attempts + 1);
}

#[tokio::test(start_paused = true)]
async fn disk_capacity_reader_notifications_cannot_bypass_retry_backoff() {
    let (filesystem, mut sender, _reader, _) =
        build_capacity_blocked_policy_disk(WhenFull::DropNewest).await;
    let state = match sender.get_base_ref() {
        SenderAdapter::DiskV2Test(state) => Arc::clone(state),
        _ => unreachable!("test disk sender expected"),
    };
    filesystem.fail_data_writes_after(10, io::ErrorKind::StorageFull);

    sender.send(Record::new(128, 256, 1), None).await.unwrap();
    tokio::task::yield_now().await;
    let initial_attempts = filesystem.data_write_attempts();

    state.notify_reader_waiters_for_test().await;
    tokio::task::yield_now().await;
    assert_eq!(filesystem.data_write_attempts(), initial_attempts + 1);

    for _ in 0..3 {
        state.notify_reader_waiters_for_test().await;
        tokio::task::yield_now().await;
    }
    assert_eq!(filesystem.data_write_attempts(), initial_attempts + 1);

    advance(Duration::from_millis(199)).await;
    tokio::task::yield_now().await;
    assert_eq!(filesystem.data_write_attempts(), initial_attempts + 1);
    advance(Duration::from_millis(1)).await;
    tokio::task::yield_now().await;
    assert_eq!(filesystem.data_write_attempts(), initial_attempts + 2);

    state.notify_reader_waiters_for_test().await;
    tokio::task::yield_now().await;
    assert_eq!(filesystem.data_write_attempts(), initial_attempts + 3);
}

#[tokio::test]
async fn disk_capacity_send_publishes_retrying_before_releasing_writer() {
    let (filesystem, mut sender, _reader, _) =
        build_capacity_blocked_policy_disk(WhenFull::DropNewest).await;
    let state = match sender.get_base_ref() {
        SenderAdapter::DiskV2Test(state) => Arc::clone(state),
        _ => unreachable!("test disk sender expected"),
    };
    let hook = Arc::new(CapacityBlockedHook::default());
    state.set_capacity_blocked_hook(Arc::clone(&hook));
    filesystem.fail_data_writes_after(10, io::ErrorKind::StorageFull);

    let detected = hook.detected.notified();
    tokio::pin!(detected);
    detected.as_mut().enable();
    let send = tokio::spawn(async move { sender.send(Record::new(127, 256, 1), None).await });
    detected.await;
    let write_attempts = filesystem.data_write_attempts();

    let mut flusher = BufferSender::new(SenderAdapter::DiskV2Test(state), WhenFull::DropNewest);
    let flush = tokio::spawn(async move { flusher.flush().await });
    tokio::task::yield_now().await;
    hook.resume.notify_waiters();

    send.await.unwrap().unwrap();
    flush.await.unwrap().unwrap();
    assert_eq!(filesystem.data_write_attempts(), write_attempts);
}

#[tokio::test]
async fn disk_capacity_overflow_routes_from_cloned_sender() {
    let (filesystem, mut sender, mut base_reader, base_usage) =
        build_capacity_blocked_policy_disk(WhenFull::DropNewest).await;
    let (overflow_sender, mut overflow_reader, overflow_usage) =
        build_policy_disk(TestFilesystem::default(), 1024, false, WhenFull::Block).await;
    sender.switch_to_overflow(overflow_sender);
    let mut clone = sender.clone();
    let owned = Record::new(103, 256, 1);
    let overflowed = Record::new(104, 64, 1);
    filesystem.fail_data_writes_after(10, io::ErrorKind::StorageFull);

    timeout(Duration::from_secs(1), sender.send(owned.clone(), None))
        .await
        .expect("the owned overflow record should be accepted by the base")
        .unwrap();
    timeout(
        Duration::from_millis(20),
        clone.send(overflowed.clone(), None),
    )
    .await
    .expect("cloned overflow sender must not wait for the base retry")
    .unwrap();
    clone.flush().await.unwrap();
    wait_for_received_events(
        &overflow_usage,
        1,
        Duration::from_secs(1),
        "overflowed record should be flushed",
    )
    .await;
    assert_eq!(overflow_reader.next().await.unwrap(), Some(overflowed));

    filesystem.restore_data_writes();
    wait_for_received_events(
        &base_usage,
        1,
        Duration::from_secs(2),
        "base owned record should recover",
    )
    .await;
    assert_eq!(base_reader.next().await.unwrap(), Some(owned));
}

#[tokio::test(start_paused = true)]
async fn disk_capacity_overflow_flushes_base_without_retrying_writer() {
    let (filesystem, mut sender, _base_reader, _) =
        build_capacity_blocked_policy_disk(WhenFull::DropNewest).await;
    let (overflow_sender, mut overflow_reader, overflow_usage) =
        build_policy_disk(TestFilesystem::default(), 1024, false, WhenFull::Block).await;
    sender.switch_to_overflow(overflow_sender);
    filesystem.fail_data_writes_after(10, io::ErrorKind::StorageFull);

    sender.send(Record::new(124, 256, 1), None).await.unwrap();
    tokio::task::yield_now().await;
    let write_attempts = filesystem.data_write_attempts();
    let overflowed = Record::new(125, 64, 1);
    sender.send(overflowed.clone(), None).await.unwrap();
    sender.flush().await.unwrap();

    assert_eq!(filesystem.data_write_attempts(), write_attempts);
    assert_eq!(overflow_usage.snapshot().received_event_count, 1);
    assert_eq!(overflow_reader.next().await.unwrap(), Some(overflowed));
}

#[tokio::test(start_paused = true)]
async fn disk_capacity_blocking_flush_waits_then_recovers() {
    let (filesystem, mut owner, mut reader, _) =
        build_capacity_blocked_policy_disk(WhenFull::DropNewest).await;
    let state = match owner.get_base_ref() {
        SenderAdapter::DiskV2Test(state) => Arc::clone(state),
        _ => unreachable!("test disk sender expected"),
    };
    let mut blocker = BufferSender::new(SenderAdapter::DiskV2Test(state), WhenFull::Block);
    let record = Record::new(126, 256, 1);
    filesystem.fail_data_writes_after(10, io::ErrorKind::StorageFull);
    owner.send(record.clone(), None).await.unwrap();
    tokio::task::yield_now().await;
    let write_attempts = filesystem.data_write_attempts();

    let mut flush = Box::pin(blocker.flush());
    tokio::select! {
        result = &mut flush => panic!("capacity-blocked flush unexpectedly completed: {result:?}"),
        () = tokio::task::yield_now() => {}
    }
    assert_eq!(filesystem.data_write_attempts(), write_attempts);

    filesystem.restore_data_writes();
    advance(Duration::from_millis(100)).await;
    flush.await.unwrap();
    assert_eq!(reader.next().await.unwrap(), Some(record));
}

#[tokio::test]
async fn disk_retry_waiting_for_reader_releases_sender_mutex() {
    let filesystem = TestFilesystem::default();
    let (mut sender, _reader, _) =
        build_policy_disk(filesystem.clone(), 1024, true, WhenFull::DropNewest).await;
    for id in 105..107 {
        sender.send(Record::new(id, 64, 1), None).await.unwrap();
        sender.flush().await.unwrap();
    }
    let state = match sender.get_base_ref() {
        SenderAdapter::DiskV2Test(state) => Arc::clone(state),
        _ => unreachable!("test disk sender expected"),
    };
    let next_path = state.next_writer_data_file_path().await;
    filesystem.fail_data_file_open(io::ErrorKind::StorageFull);
    sender.send(Record::new(107, 64, 1), None).await.unwrap();
    filesystem.restore_data_file_open();
    filesystem.create_data_file_with_data(&next_path, b"occupied");
    sleep(Duration::from_millis(150)).await;

    let mut clone = sender.clone();
    timeout(
        Duration::from_millis(20),
        clone.send(Record::new(108, 64, 1), None),
    )
    .await
    .expect("retry waiting for reader must not hold the sender mutex")
    .unwrap();
}

#[tokio::test]
async fn ordinary_capacity_retry_does_not_retain_dropped_sender() {
    let (filesystem, mut sender, reader, _) =
        build_capacity_blocked_policy_disk(WhenFull::DropNewest).await;
    let weak = match sender.get_base_ref() {
        SenderAdapter::DiskV2Test(state) => Arc::downgrade(state),
        _ => unreachable!("test disk sender expected"),
    };
    filesystem.fail_data_writes_after(10, io::ErrorKind::StorageFull);
    sender.send(Record::new(109, 256, 1), None).await.unwrap();

    drop(sender);
    drop(reader);
    timeout(Duration::from_millis(100), async {
        while weak.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("ordinary retry task must not retain the disk sender or writer");
}

#[tokio::test]
async fn cancelling_unowned_block_send_does_not_detach_or_duplicate_record() {
    let (filesystem, mut owner, mut reader, usage) =
        build_capacity_blocked_policy_disk(WhenFull::DropNewest).await;
    let state = match owner.get_base_ref() {
        SenderAdapter::DiskV2Test(state) => Arc::clone(state),
        _ => unreachable!("test disk sender expected"),
    };
    let weak = Arc::downgrade(&state);
    let mut blocker = BufferSender::new(SenderAdapter::DiskV2Test(state), WhenFull::Block);
    let pending_record = Record::new(110, 256, 1);
    let mut cancelled_record = Record::new(111, 64, 1);
    let (batch, finalizer) = BatchNotifier::new_with_receiver();
    cancelled_record.add_batch_notifier(batch);
    filesystem.fail_data_writes_after(10, io::ErrorKind::StorageFull);

    owner.send(pending_record.clone(), None).await.unwrap();
    let send = tokio::spawn(async move { blocker.send(cancelled_record, None).await });
    sleep(Duration::from_millis(20)).await;
    assert!(
        !send.is_finished(),
        "block send should be waiting for capacity"
    );
    send.abort();
    assert!(send.await.unwrap_err().is_cancelled());
    assert_eq!(
        timeout(Duration::from_secs(1), finalizer).await.unwrap(),
        BatchStatus::Errored,
        "cancelling an unowned send must nack it for source redelivery"
    );

    filesystem.restore_data_writes();
    wait_for_received_events(
        &usage,
        1,
        Duration::from_secs(2),
        "writer-owned record should complete in the background",
    )
    .await;
    assert_eq!(reader.next().await.unwrap(), Some(pending_record));
    assert!(
        timeout(Duration::from_millis(50), reader.next())
            .await
            .is_err(),
        "the cancelled unowned record must not be written later"
    );

    drop(owner);
    drop(reader);
    timeout(Duration::from_millis(100), async {
        while weak.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cancelled block send must not retain the disk sender or writer");
}

#[tokio::test]
async fn terminal_capacity_retry_wakes_waiters_and_stays_failed() {
    let (filesystem, mut owner, reader, _) =
        build_capacity_blocked_policy_disk(WhenFull::DropNewest).await;
    let state = match owner.get_base_ref() {
        SenderAdapter::DiskV2Test(state) => Arc::clone(state),
        _ => unreachable!("test disk sender expected"),
    };
    let mut blocker = BufferSender::new(SenderAdapter::DiskV2Test(state), WhenFull::Block);
    filesystem.fail_data_writes_after(10, io::ErrorKind::StorageFull);
    owner.send(Record::new(112, 256, 1), None).await.unwrap();

    let waiting = tokio::spawn(async move { blocker.send(Record::new(113, 64, 1), None).await });
    sleep(Duration::from_millis(20)).await;
    assert!(!waiting.is_finished(), "block send should await capacity");
    #[cfg(unix)]
    filesystem.fail_data_writes_after_raw_os_error(0, libc::EACCES);
    #[cfg(not(unix))]
    filesystem.fail_data_writes_after(0, io::ErrorKind::PermissionDenied);
    assert_terminal_permission_denied(
        timeout(Duration::from_secs(1), waiting)
            .await
            .expect("terminal retry must wake the waiter")
            .unwrap()
            .expect_err("terminal retry must fail the waiter"),
    );

    let mut rejected = Record::new(114, 64, 1);
    let (batch, finalizer) = BatchNotifier::new_with_receiver();
    rejected.add_batch_notifier(batch);
    assert_terminal_permission_denied(
        owner
            .send(rejected, None)
            .await
            .expect_err("terminal retry state must reject subsequent sends"),
    );
    assert_eq!(
        timeout(Duration::from_secs(1), finalizer)
            .await
            .expect("terminal send finalizer should resolve"),
        BatchStatus::Errored,
        "terminal sends must nack finalizers for source redelivery"
    );
    assert_terminal_permission_denied(
        owner
            .flush()
            .await
            .expect_err("terminal retry state must reject flushes"),
    );
    drop(owner);
    drop(reader);
}

#[tokio::test]
async fn direct_pending_fatal_write_nacks_and_permanently_fails_sender() {
    let filesystem = TestFilesystem::default();
    filesystem.set_max_write_size(Some(5));
    let (mut sender, mut reader, _) =
        build_policy_disk(filesystem.clone(), 16, false, WhenFull::Block).await;
    let mut record = Record::new(129, 256, 1);
    let (batch, finalizer) = BatchNotifier::new_with_receiver();
    record.add_batch_notifier(batch);
    #[cfg(unix)]
    filesystem.fail_data_writes_after_raw_os_error(10, libc::EACCES);
    #[cfg(not(unix))]
    filesystem.fail_data_writes_after(10, io::ErrorKind::PermissionDenied);

    assert_writer_permission_denied(
        timeout(Duration::from_secs(1), sender.send(record, None))
            .await
            .expect("partial direct write should terminate")
            .expect_err("partial direct write must return its fatal error"),
    );
    assert_eq!(
        timeout(Duration::from_secs(1), finalizer)
            .await
            .expect("pending record finalizer should resolve"),
        BatchStatus::Errored
    );

    filesystem.restore_data_writes();
    let mut rejected = Record::new(130, 64, 1);
    let (batch, rejected_finalizer) = BatchNotifier::new_with_receiver();
    rejected.add_batch_notifier(batch);
    assert_terminal_permission_denied(
        sender
            .send(rejected, None)
            .await
            .expect_err("failed sender must reject later sends"),
    );
    assert_eq!(
        timeout(Duration::from_secs(1), rejected_finalizer)
            .await
            .expect("rejected record finalizer should resolve"),
        BatchStatus::Errored
    );
    assert_terminal_permission_denied(
        sender
            .flush()
            .await
            .expect_err("failed sender must reject later flushes"),
    );
    drop(sender);
    assert_eq!(
        timeout(Duration::from_secs(1), reader.next())
            .await
            .expect("closed failed buffer should reach EOF")
            .expect("failed partial data should not produce a reader error"),
        None,
        "the failed pending record must never be delivered"
    );
}

#[tokio::test]
async fn terminal_cancellation_retry_is_not_overwritten_by_queued_ready_retry() {
    let (filesystem, mut sender, _reader, _) =
        build_capacity_blocked_policy_disk(WhenFull::DropNewest).await;
    let state = match sender.get_base_ref() {
        SenderAdapter::DiskV2Test(state) => Arc::clone(state),
        _ => unreachable!("test disk sender expected"),
    };
    let mut record = Record::new(114, 256, 1);
    let (batch, mut finalizer) = BatchNotifier::new_with_receiver();
    record.add_batch_notifier(batch);
    filesystem.fail_data_writes_after(10, io::ErrorKind::StorageFull);
    timeout(Duration::from_secs(1), sender.send(record, None))
        .await
        .expect("drop-newest should accept the writer-owned record")
        .unwrap();

    // Let the ordinary weak retry observe StorageFull and schedule its later retry.
    sleep(Duration::from_millis(150)).await;
    filesystem.fail_data_writes_after(0, io::ErrorKind::PermissionDenied);
    state.start_cancellation_retry_for_test();
    assert_eq!(
        timeout(Duration::from_secs(1), &mut finalizer)
            .await
            .expect("terminal classification should resolve the owned record once"),
        BatchStatus::Errored
    );

    // The queued ordinary driver would get Ready from the restored filesystem if it did not
    // recheck the terminal state while holding the writer mutex.
    filesystem.restore_data_writes();
    sleep(Duration::from_millis(250)).await;
    assert!(
        sender.send(Record::new(115, 64, 1), None).await.is_err(),
        "a stale Ready retry must not overwrite Failed"
    );
}

#[tokio::test]
async fn cancelled_production_send_from_ready_retains_owner_until_syscall_is_classified() {
    let directory = TempDir::with_prefix("vector-buffer-adapter-cancel").unwrap();
    let data_dir = directory.path().to_path_buf();
    let StalledWrites { filesystem, gate } = ProductionFilesystem::with_stalled_writes();
    let _gate_cleanup = WriteGateCleanup(Arc::clone(&gate));
    let config = DiskBufferConfigBuilder::from_path(&data_dir)
        .write_buffer_size(16)
        .filesystem(filesystem.clone())
        .build()
        .unwrap();
    let (writer, reader, _) = Buffer::from_config_inner(config, BufferUsageHandle::noop())
        .await
        .unwrap();
    let mut sender = BufferSender::new(SenderAdapter::from(writer), WhenFull::Block);
    let weak = match sender.get_base_ref() {
        SenderAdapter::DiskV2(state) => Arc::downgrade(state),
        _ => unreachable!("production disk sender expected"),
    };
    let mut record = Record::new(115, 256, 1);
    let expected = record.clone();
    let (batch, mut finalizer) = BatchNotifier::new_with_receiver();
    record.add_batch_notifier(batch);

    let mut send = Box::pin(sender.send(record, None));
    tokio::select! {
        result = &mut send => panic!("gated send unexpectedly completed: {result:?}"),
        () = gate.wait_until_started() => {}
    }
    drop(send);
    // Cancellation registers its retained driver before a normal starter can observe Retrying.
    match sender.get_base_ref() {
        SenderAdapter::DiskV2(state) => state.start_normal_capacity_retry_for_test(),
        _ => unreachable!("production disk sender expected"),
    }
    drop(sender);
    drop(reader);
    assert!(
        weak.upgrade().is_some(),
        "the cancellation retry must retain the sender while its syscall is gated"
    );
    assert!(
        timeout(Duration::from_millis(20), &mut finalizer)
            .await
            .is_err(),
        "writer-owned finalizers must remain unresolved while the syscall is pending"
    );

    gate.release();
    assert_eq!(
        timeout(Duration::from_secs(2), &mut finalizer)
            .await
            .expect("the recovered write should resolve its finalizer"),
        BatchStatus::Delivered
    );
    timeout(Duration::from_millis(100), async {
        while weak.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the retry must release the sender after classifying the syscall");

    let config = DiskBufferConfigBuilder::from_path(&data_dir)
        .write_buffer_size(16)
        .filesystem(filesystem)
        .build()
        .unwrap();
    let (recovered_writer, mut recovered_reader, _) =
        Buffer::from_config_inner(config, BufferUsageHandle::noop())
            .await
            .unwrap();
    drop(recovered_writer);
    assert_eq!(
        timeout(Duration::from_secs(2), recovered_reader.next())
            .await
            .expect("the recovered record should be readable")
            .unwrap(),
        Some(expected)
    );
    assert_eq!(
        timeout(Duration::from_secs(2), recovered_reader.next())
            .await
            .expect("the recovered buffer should reach EOF")
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn cancelled_production_flush_while_retrying_retains_owner_until_classified() {
    let directory = TempDir::with_prefix("vector-buffer-adapter-retrying-cancel").unwrap();
    let data_dir = directory.path().to_path_buf();
    let StalledWrites { filesystem, gate } = ProductionFilesystem::with_stalled_writes();
    let _gate_cleanup = WriteGateCleanup(Arc::clone(&gate));
    let config = DiskBufferConfigBuilder::from_path(&data_dir)
        .write_buffer_size(1024)
        .filesystem(filesystem.clone())
        .build()
        .unwrap();
    let (writer, reader, _) = Buffer::from_config_inner(config, BufferUsageHandle::noop())
        .await
        .unwrap();
    let mut sender = BufferSender::new(SenderAdapter::from(writer), WhenFull::Block);
    let weak = match sender.get_base_ref() {
        SenderAdapter::DiskV2(state) => Arc::downgrade(state),
        _ => unreachable!("production disk sender expected"),
    };
    let mut record = Record::new(119, 256, 1);
    let expected = record.clone();
    let (batch, mut finalizer) = BatchNotifier::new_with_receiver();
    record.add_batch_notifier(batch);
    sender.send(record, None).await.unwrap();
    match sender.get_base_ref() {
        SenderAdapter::DiskV2(state) => state.start_normal_capacity_retry_for_test(),
        _ => unreachable!("production disk sender expected"),
    }

    let mut flush = Box::pin(sender.flush());
    tokio::select! {
        result = &mut flush => panic!("gated flush unexpectedly completed: {result:?}"),
        () = gate.wait_until_started() => {}
    }
    drop(flush);
    drop(sender);
    drop(reader);
    assert!(
        weak.upgrade().is_some(),
        "the cancellation classifier must retain the sender while its syscall is gated"
    );

    gate.release();
    assert_eq!(
        timeout(Duration::from_secs(2), &mut finalizer)
            .await
            .expect("the accepted record should resolve its finalizer"),
        BatchStatus::Delivered
    );
    timeout(Duration::from_secs(2), async {
        while weak.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the classifier must release the sender after classifying the syscall");

    let config = DiskBufferConfigBuilder::from_path(&data_dir)
        .write_buffer_size(1024)
        .filesystem(filesystem)
        .build()
        .unwrap();
    let (recovered_writer, mut recovered_reader, _) =
        Buffer::from_config_inner(config, BufferUsageHandle::noop())
            .await
            .unwrap();
    drop(recovered_writer);
    assert_eq!(
        timeout(Duration::from_secs(2), recovered_reader.next())
            .await
            .expect("the recovered record should be readable")
            .unwrap(),
        Some(expected)
    );
    assert_eq!(
        timeout(Duration::from_secs(2), recovered_reader.next())
            .await
            .expect("the recovered buffer should reach EOF")
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn cancelled_production_flush_recovers_accepted_record() {
    let directory = TempDir::with_prefix("vector-buffer-adapter-flush-cancel").unwrap();
    let StalledWrites { filesystem, gate } = ProductionFilesystem::with_stalled_writes();
    let _gate_cleanup = WriteGateCleanup(Arc::clone(&gate));
    let config = DiskBufferConfigBuilder::from_path(directory.path())
        .write_buffer_size(1024)
        .filesystem(filesystem)
        .build()
        .unwrap();
    let (writer, mut reader, _) = Buffer::from_config_inner(config, BufferUsageHandle::noop())
        .await
        .unwrap();
    let mut sender = BufferSender::new(SenderAdapter::from(writer), WhenFull::Block);
    let record = Record::new(118, 256, 1);

    sender.send(record.clone(), None).await.unwrap();
    let mut flush = Box::pin(sender.flush());
    tokio::select! {
        result = &mut flush => panic!("gated flush unexpectedly completed: {result:?}"),
        () = gate.wait_until_started() => {}
    }
    drop(flush);
    gate.release();

    assert_eq!(
        timeout(Duration::from_secs(2), reader.next())
            .await
            .expect("cancelled flush should be recovered in the background")
            .unwrap(),
        Some(record)
    );
}
