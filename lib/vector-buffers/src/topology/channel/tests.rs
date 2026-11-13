use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::{pin, sync::Barrier, time::sleep};

use crate::{
    Bufferable, WhenFull,
    topology::{
        channel::{BufferReceiver, BufferSender},
        test_util::{assert_current_send_capacity, build_buffer},
    },
};

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

/// Memory -> disk overflow: records spilled to the disk stage during an outage must be delivered
/// while the memory stage keeps receiving live traffic, without a restart.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn overflow_disk_stage_drains_while_memory_stage_is_live() {
    use std::{collections::HashSet, num::NonZeroU64, num::NonZeroUsize};

    use tracing::Span;

    use crate::{
        config::MemoryBufferSize,
        test::with_temp_dir,
        topology::{builder::TopologyBuilder, test_util::Sample},
        variants::{DiskV2Buffer, MemoryBuffer},
    };

    // Each record is ~320 KiB, larger than the disk reader's 256 KiB read buffer, so every
    // record needs more than one `fill_buf` to be read.
    const VALUES: usize = 40_000;
    const SPILLED: u64 = 60;
    const LIVE: u64 = 2_000;

    fn record(id: u64) -> Sample {
        let mut v = vec![0u64; VALUES];
        v[0] = id;
        Sample::HeapAllocatedValues(v)
    }
    fn id_of(s: &Sample) -> u64 {
        match s {
            Sample::HeapAllocatedValues(v) => v[0],
            Sample::SingleValue(v) => *v,
        }
    }

    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();
        async move {
            let mut builder = TopologyBuilder::<Sample>::default();
            builder.stage(
                MemoryBuffer::new(MemoryBufferSize::MaxEvents(NonZeroUsize::new(4).unwrap())),
                WhenFull::Overflow,
            );
            builder.stage(
                DiskV2Buffer::new(
                    "overflow".to_string(),
                    data_dir,
                    NonZeroU64::new(1 << 30).unwrap(),
                ),
                WhenFull::Block,
            );
            let (mut tx, mut rx) = builder
                .build("overflow".to_string(), Span::none())
                .await
                .expect("build");

            // Outage: nobody reads, so everything past the memory stage spills to disk.
            for id in 0..SPILLED {
                tx.send(record(id), None).await.unwrap();
                tx.flush().await.unwrap();
            }

            // Recovery: live traffic keeps the memory stage busy while the sink drains.
            let producer = tokio::spawn(async move {
                for id in SPILLED..SPILLED + LIVE {
                    tx.send(Sample::SingleValue(id), None).await.unwrap();
                    tx.flush().await.unwrap();
                    tokio::task::yield_now().await;
                }
                tx
            });

            let mut seen = HashSet::new();
            let deadline = Instant::now() + Duration::from_secs(30);
            while (seen.len() as u64) < SPILLED + LIVE && Instant::now() < deadline {
                match tokio::time::timeout(Duration::from_secs(2), rx.next()).await {
                    Ok(Some(item)) => {
                        seen.insert(id_of(&item));
                    }
                    Ok(None) | Err(_) => break,
                }
            }
            let _tx = producer.await.unwrap();

            let missing_spilled = (0..SPILLED).filter(|id| !seen.contains(id)).count();
            let missing_live = (SPILLED..SPILLED + LIVE).filter(|id| !seen.contains(id)).count();
            assert_eq!(
                (missing_spilled, missing_live),
                (0, 0),
                "spilled records missing: {missing_spilled}/{SPILLED}, live missing: {missing_live}/{LIVE}"
            );
        }
    })
    .await;
}
