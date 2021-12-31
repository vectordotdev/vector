use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use futures::{SinkExt, StreamExt};
use tokio::{pin, sync::Barrier, time::sleep};

use crate::{
    topology::{
        channel::{BufferReceiver, BufferSender},
        test_util::{assert_current_send_capacity, build_buffer},
    },
    Bufferable, WhenFull,
};

async fn assert_send_ok_with_capacities<T>(
    sender: &mut BufferSender<T>,
    value: T,
    base_expected: Option<usize>,
    overflow_expected: Option<usize>,
) where
    T: Bufferable,
{
    assert!(sender.send(value).await.is_ok());
    assert_current_send_capacity(sender, base_expected, overflow_expected);
}

async fn blocking_send_and_drain_receiver<T>(
    mut sender: BufferSender<T>,
    receiver: BufferReceiver<T>,
    send_value: T,
) -> Vec<T>
where
    T: Bufferable,
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
        let _ = recv_baton.wait().await;
        sleep(recv_delay).await;

        // Grab all messages and then return the results.
        while let Some(msg) = receiver.next().await {
            results.push(msg);
        }
        results
    });

    // We also have to drop our sender after sending the fourth message so that the receiver
    // task correctly exits.  If we didn't drop it, the receiver task would just assume that we
    // had no more messages to send, waiting for-ev-er for the next one.
    let start = Instant::now();
    let _ = send_baton.wait().await;
    assert!(sender.send(send_value).await.is_ok());
    let send_delay = start.elapsed();
    assert!(send_delay > recv_delay);
    drop(sender);

    handle.await.expect("receiver task should not panic")
}

async fn drain_receiver<T>(sender: BufferSender<T>, receiver: BufferReceiver<T>) -> Vec<T>
where
    T: Bufferable,
{
    drop(sender);
    let handle = tokio::spawn(async move {
        let mut results = Vec::new();
        pin!(receiver);

        // Grab all messages and then return the results.
        while let Some(msg) = receiver.next().await {
            results.push(msg);
        }
        results
    });

    handle.await.expect("receiver task should not panic")
}

#[tokio::test]
async fn test_sender_block() {
    // Get a non-overflow buffer in blocking mode with a capacity of 3.
    let (mut tx, rx) = build_buffer(3, WhenFull::Block, None).await;

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
    let (mut tx, rx) = build_buffer(3, WhenFull::DropNewest, None).await;

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
    let mut results = drain_receiver(tx, rx).await;
    results.sort_unstable();
    assert_eq!(results, vec![1, 2, 3]);
}

#[tokio::test]
async fn test_sender_overflow_block() {
    // Get an overflow buffer, where the overflow buffer is in blocking mode, and both the base
    // and overflow buffers have a capacity of 2.
    let (mut tx, rx) = build_buffer(2, WhenFull::Overflow, Some(WhenFull::Block)).await;

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
    let (mut tx, rx) = build_buffer(2, WhenFull::Overflow, Some(WhenFull::DropNewest)).await;

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
    let mut results = drain_receiver(tx, rx).await;
    results.sort_unstable();
    assert_eq!(results, vec![1, 2, 7, 8]);
}
