use std::{
    io,
    sync::Arc,
    time::{Duration, Instant},
};

use temp_dir::TempDir;
use tokio::{
    pin,
    sync::Barrier,
    time::{advance, sleep, timeout},
};
use vector_common::finalization::{AddBatchNotifier, BatchNotifier, BatchStatus};

use crate::{
    Bufferable, WhenFull,
    buffer_usage_data::BufferUsageHandle,
    topology::{
        channel::{BufferReceiver, BufferSender, CapacityBlockedHook, SenderAdapter},
        test_util::{assert_current_send_capacity, build_buffer},
    },
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

async fn assert_send_ok_with_capacities<T>(
    sender: &mut BufferSender<T>,
    value: impl Into<T>,
    base_expected: Option<usize>,
    overflow_expected: Option<usize>,
) where
    T: Bufferable,
{
    assert!(sender.send(value.into(), None).await.is_ok());
    assert_current_send_capacity(sender, base_expected, overflow_expected);
}

async fn blocking_send_and_drain_receiver<T, V>(
    mut sender: BufferSender<T>,
    receiver: BufferReceiver<T>,
    send_value: V,
) -> Vec<V>
where
    T: Bufferable,
    V: Into<T> + From<T> + Send + 'static,
{
    // We can likely replace this with `tokio_test`-related helpers to avoid the sleeping.
    let send_baton = Arc::new(Barrier::new(2));
    let recv_baton = Arc::clone(&send_baton);
    let recv_delay = Duration::from_millis(500);
    let handle = tokio::spawn(async move {
        let mut results = Vec::new();
        pin!(receiver);

        // Synchronize with sender and then wait for a small period of time to simulate a
        // blocking delay.
        _ = recv_baton.wait().await;
        sleep(recv_delay).await;

        // Grab all messages and then return the results.
        while let Some(msg) = receiver.next().await {
            results.push(msg.into());
        }
        results
    });

    // We also have to drop our sender after sending the fourth message so that the receiver
    // task correctly exits.  If we didn't drop it, the receiver task would just assume that we
    // had no more messages to send, waiting for-ev-er for the next one.
    let start = Instant::now();
    _ = send_baton.wait().await;
    assert!(sender.send(send_value.into(), None).await.is_ok());
    let send_delay = start.elapsed();
    assert!(send_delay > recv_delay);
    drop(sender);

    handle.await.expect("receiver task should not panic")
}

async fn drain_receiver<T, V>(sender: BufferSender<T>, receiver: BufferReceiver<T>) -> Vec<V>
where
    T: Bufferable,
    V: From<T> + Send + 'static,
{
    drop(sender);
    let handle = tokio::spawn(async move {
        let mut results = Vec::new();
        pin!(receiver);

        // Grab all messages and then return the results.
        while let Some(msg) = receiver.next().await {
            results.push(msg.into());
        }
        results
    });

    handle.await.expect("receiver task should not panic")
}

#[tokio::test]
async fn test_sender_block() {
    // Get a non-overflow buffer in blocking mode with a capacity of 3.
    let (mut tx, rx, _) = build_buffer(3, WhenFull::Block, None);

    // We should be able to send three messages through unimpeded.
    assert_current_send_capacity(&mut tx, Some(3), None);
    assert_send_ok_with_capacities(&mut tx, 1, Some(2), None).await;
    assert_send_ok_with_capacities(&mut tx, 2, Some(1), None).await;
    assert_send_ok_with_capacities(&mut tx, 3, Some(0), None).await;

    // Our next send _should_ block.  `assert_sender_blocking_send_and_recv` spawns a receiver
    // task which waits for a small period of time, and we track how long our next send blocks
    // for, which should be greater than the time that the receiver task waits.  This asserts
    // that the send is blocking, and that it's dependent on the receiver.
    //
    // It also drops the sender and receives all remaining messages on the receiver, returning
    // them to us to check.
    let mut results = blocking_send_and_drain_receiver(tx, rx, 4).await;
    results.sort_unstable();
    assert_eq!(results, vec![1, 2, 3, 4]);
}

#[tokio::test]
async fn test_sender_drop_newest() {
    // Get a non-overflow buffer in "drop newest" mode with a capacity of 3.
    let (mut tx, rx, _) = build_buffer(3, WhenFull::DropNewest, None);

    // We should be able to send three messages through unimpeded.
    assert_current_send_capacity(&mut tx, Some(3), None);
    assert_send_ok_with_capacities(&mut tx, 1, Some(2), None).await;
    assert_send_ok_with_capacities(&mut tx, 2, Some(1), None).await;
    assert_send_ok_with_capacities(&mut tx, 3, Some(0), None).await;

    // Then, since we're in "drop newest" mode, we could continue to send without issue or being
    // blocked, but we would except those items to, well.... be dropped.
    assert_send_ok_with_capacities(&mut tx, 7, Some(0), None).await;
    assert_send_ok_with_capacities(&mut tx, 8, Some(0), None).await;
    assert_send_ok_with_capacities(&mut tx, 9, Some(0), None).await;

    // Then, when we collect all of the messages from the receiver, we should only get back the
    // first three of them.
    let mut results: Vec<u64> = drain_receiver(tx, rx).await;
    results.sort_unstable();
    assert_eq!(results, vec![1, 2, 3]);
}

#[tokio::test]
async fn test_sender_overflow_block() {
    // Get an overflow buffer, where the overflow buffer is in blocking mode, and both the base
    // and overflow buffers have a capacity of 2.
    let (mut tx, rx, _) = build_buffer(2, WhenFull::Overflow, Some(WhenFull::Block));

    // We should be able to send four message through unimpeded -- two for the base sender, and
    // two for the overflow sender.
    assert_current_send_capacity(&mut tx, Some(2), Some(2));
    assert_send_ok_with_capacities(&mut tx, 1, Some(1), Some(2)).await;
    assert_send_ok_with_capacities(&mut tx, 2, Some(0), Some(2)).await;
    assert_send_ok_with_capacities(&mut tx, 3, Some(0), Some(1)).await;
    assert_send_ok_with_capacities(&mut tx, 4, Some(0), Some(0)).await;

    // Our next send _should_ block.  `assert_sender_blocking_send_and_recv` spawns a receiver
    // task which waits for a small period of time, and we track how long our next send blocks
    // for, which should be greater than the time that the receiver task waits.  This asserts
    // that the send is blocking, and that it's dependent on the receiver.
    //
    // It also drops the sender and receives all remaining messages on the receiver, returning
    // them to us to check.
    let mut results = blocking_send_and_drain_receiver(tx, rx, 5).await;
    results.sort_unstable();
    assert_eq!(results, vec![1, 2, 3, 4, 5]);
}

#[tokio::test]
async fn test_sender_overflow_drop_newest() {
    // Get an overflow buffer, where the overflow buffer is in "drop newest" mode, and both the
    // base and overflow buffers have a capacity of 2.
    let (mut tx, rx, _) = build_buffer(2, WhenFull::Overflow, Some(WhenFull::DropNewest));

    // We should be able to send four message through unimpeded -- two for the base sender, and
    // two for the overflow sender.
    assert_current_send_capacity(&mut tx, Some(2), Some(2));
    assert_send_ok_with_capacities(&mut tx, 7, Some(1), Some(2)).await;
    assert_send_ok_with_capacities(&mut tx, 8, Some(0), Some(2)).await;
    assert_send_ok_with_capacities(&mut tx, 2, Some(0), Some(1)).await;
    assert_send_ok_with_capacities(&mut tx, 1, Some(0), Some(0)).await;

    // Then, since we're in "drop newest" mode on the overflow side, we could continue to send
    // without issue or being blocked, but we would except those items to, well.... be dropped.
    assert_send_ok_with_capacities(&mut tx, 5, Some(0), Some(0)).await;
    assert_send_ok_with_capacities(&mut tx, 6, Some(0), Some(0)).await;
    assert_send_ok_with_capacities(&mut tx, 3, Some(0), Some(0)).await;

    // Then, when we collect all of the messages from the receiver, we should only get back the
    // first four of them.
    let mut results: Vec<u64> = drain_receiver(tx, rx).await;
    results.sort_unstable();
    assert_eq!(results, vec![1, 2, 7, 8]);
}

#[tokio::test]
async fn test_buffer_metrics_normal() {
    // Get a regular blocking buffer.
    let (mut tx, rx, handle) = build_buffer(5, WhenFull::Block, None);

    // Send three items through, and make sure the buffer usage stats reflect that.
    assert_current_send_capacity(&mut tx, Some(5), None);
    assert_send_ok_with_capacities(&mut tx, 7, Some(4), None).await;
    assert_send_ok_with_capacities(&mut tx, 8, Some(3), None).await;
    assert_send_ok_with_capacities(&mut tx, 2, Some(2), None).await;

    let snapshot = handle.snapshot();
    assert_eq!(3, snapshot.received_event_count);
    assert_eq!(0, snapshot.sent_event_count);
    assert_eq!(0, snapshot.dropped_event_count_intentional);

    // Then, when we collect all of the messages from the receiver, the metrics should also reflect that.
    let mut results: Vec<u64> = drain_receiver(tx, rx).await;
    results.sort_unstable();
    assert_eq!(results, vec![2, 7, 8]);

    let snapshot = handle.snapshot();
    assert_eq!(3, snapshot.received_event_count);
    assert_eq!(3, snapshot.sent_event_count);
    assert_eq!(0, snapshot.dropped_event_count_intentional);
}

#[tokio::test]
async fn test_buffer_metrics_drop_newest() {
    // Get a buffer that drops the newest items when full.
    let (mut tx, rx, handle) = build_buffer(2, WhenFull::DropNewest, None);

    // Send three items through, and make sure the buffer usage stats reflect that.
    assert_current_send_capacity(&mut tx, Some(2), None);
    assert_send_ok_with_capacities(&mut tx, 7, Some(1), None).await;
    assert_send_ok_with_capacities(&mut tx, 8, Some(0), None).await;
    assert_send_ok_with_capacities(&mut tx, 2, Some(0), None).await;

    let snapshot = handle.snapshot();
    assert_eq!(3, snapshot.received_event_count);
    assert_eq!(0, snapshot.sent_event_count);
    assert_eq!(1, snapshot.dropped_event_count_intentional);

    // Then, when we collect all of the messages from the receiver, the metrics should also reflect that.
    let mut results: Vec<u64> = drain_receiver(tx, rx).await;
    results.sort_unstable();
    assert_eq!(results, vec![7, 8]);

    let snapshot = handle.snapshot();
    assert_eq!(3, snapshot.received_event_count);
    assert_eq!(2, snapshot.sent_event_count);
    assert_eq!(1, snapshot.dropped_event_count_intentional);
}

#[tokio::test]
async fn test_buffer_metrics_overflow_block() {
    // Get an overflow buffer, where the overflow buffer is in blocking mode, and both the base
    // and overflow buffers have a capacity of 2.
    let (mut tx, rx, handle) = build_buffer(2, WhenFull::Overflow, Some(WhenFull::Block));

    // Send four items through, and make sure the buffer usage stats reflect each item entering
    // exactly one stage: two in the base buffer and two in the overflow buffer.
    assert_current_send_capacity(&mut tx, Some(2), Some(2));
    assert_send_ok_with_capacities(&mut tx, 7, Some(1), Some(2)).await;
    assert_send_ok_with_capacities(&mut tx, 8, Some(0), Some(2)).await;
    assert_send_ok_with_capacities(&mut tx, 2, Some(0), Some(1)).await;
    assert_send_ok_with_capacities(&mut tx, 1, Some(0), Some(0)).await;

    let snapshot = handle.snapshot();
    assert_eq!(4, snapshot.received_event_count);
    assert_eq!(0, snapshot.sent_event_count);
    assert_eq!(0, snapshot.dropped_event_count_intentional);

    // Then, when we collect all of the messages from the receiver, the metrics should also reflect that.
    let mut results: Vec<u64> = drain_receiver(tx, rx).await;
    results.sort_unstable();
    assert_eq!(results, vec![1, 2, 7, 8]);

    let snapshot = handle.snapshot();
    assert_eq!(4, snapshot.received_event_count);
    assert_eq!(4, snapshot.sent_event_count);
    assert_eq!(0, snapshot.dropped_event_count_intentional);
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

#[tokio::test]
async fn disk_capacity_block_retries_owned_record() {
    let filesystem = TestFilesystem::default();
    filesystem.set_max_write_size(Some(5));
    let (mut sender, mut reader, _) =
        build_policy_disk(filesystem.clone(), 16, false, WhenFull::Block).await;
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
    let filesystem = TestFilesystem::default();
    filesystem.set_max_write_size(Some(5));
    let (mut sender, mut reader, usage) =
        build_policy_disk(filesystem.clone(), 16, false, WhenFull::DropNewest).await;
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
    timeout(Duration::from_secs(2), async {
        while usage.snapshot().received_event_count < 2 {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("owned record should complete in the background");
    assert_eq!(reader.next().await.unwrap(), Some(owned));
    let snapshot = usage.snapshot();
    assert_eq!(snapshot.received_event_count, 2);
    assert_eq!(snapshot.dropped_event_count_intentional, 1);
}

#[tokio::test(start_paused = true)]
async fn disk_capacity_drop_newest_flush_skips_writer_while_retrying() {
    let filesystem = TestFilesystem::default();
    filesystem.set_max_write_size(Some(5));
    let (mut sender, _reader, _) =
        build_policy_disk(filesystem.clone(), 16, false, WhenFull::DropNewest).await;
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

#[tokio::test]
async fn disk_capacity_send_publishes_retrying_before_releasing_writer() {
    let filesystem = TestFilesystem::default();
    filesystem.set_max_write_size(Some(5));
    let (mut sender, _reader, _) =
        build_policy_disk(filesystem.clone(), 16, false, WhenFull::DropNewest).await;
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
    let filesystem = TestFilesystem::default();
    filesystem.set_max_write_size(Some(5));
    let (mut sender, mut base_reader, base_usage) =
        build_policy_disk(filesystem.clone(), 16, false, WhenFull::DropNewest).await;
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
    timeout(Duration::from_secs(1), async {
        while overflow_usage.snapshot().received_event_count < 1 {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("overflowed record should be flushed");
    assert_eq!(overflow_reader.next().await.unwrap(), Some(overflowed));

    filesystem.restore_data_writes();
    timeout(Duration::from_secs(2), async {
        while base_usage.snapshot().received_event_count < 1 {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("base owned record should recover");
    assert_eq!(base_reader.next().await.unwrap(), Some(owned));
}

#[tokio::test(start_paused = true)]
async fn disk_capacity_overflow_flushes_base_without_retrying_writer() {
    let filesystem = TestFilesystem::default();
    filesystem.set_max_write_size(Some(5));
    let (mut sender, _base_reader, _) =
        build_policy_disk(filesystem.clone(), 16, false, WhenFull::DropNewest).await;
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
    let filesystem = TestFilesystem::default();
    filesystem.set_max_write_size(Some(5));
    let (mut owner, mut reader, _) =
        build_policy_disk(filesystem.clone(), 16, false, WhenFull::DropNewest).await;
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
    let filesystem = TestFilesystem::default();
    filesystem.set_max_write_size(Some(5));
    let (mut sender, reader, _) =
        build_policy_disk(filesystem.clone(), 16, false, WhenFull::DropNewest).await;
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
    let filesystem = TestFilesystem::default();
    filesystem.set_max_write_size(Some(5));
    let (mut owner, mut reader, usage) =
        build_policy_disk(filesystem.clone(), 16, false, WhenFull::DropNewest).await;
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
    timeout(Duration::from_secs(2), async {
        while usage.snapshot().received_event_count < 1 {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("writer-owned record should complete in the background");
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
    let filesystem = TestFilesystem::default();
    filesystem.set_max_write_size(Some(5));
    let (mut owner, reader, _) =
        build_policy_disk(filesystem.clone(), 16, false, WhenFull::DropNewest).await;
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
    filesystem.fail_data_writes_after(0, io::ErrorKind::PermissionDenied);
    assert!(
        timeout(Duration::from_secs(1), waiting)
            .await
            .expect("terminal retry must wake the waiter")
            .unwrap()
            .is_err()
    );
    assert!(
        owner.send(Record::new(114, 64, 1), None).await.is_err(),
        "terminal retry state must reject subsequent sends"
    );
    drop(owner);
    drop(reader);
}

#[tokio::test]
async fn terminal_cancellation_retry_is_not_overwritten_by_queued_ready_retry() {
    let filesystem = TestFilesystem::default();
    filesystem.set_max_write_size(Some(5));
    let (mut sender, _reader, _) =
        build_policy_disk(filesystem.clone(), 16, false, WhenFull::DropNewest).await;
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
