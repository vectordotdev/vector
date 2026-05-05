use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::stream::{FuturesUnordered, Stream};
use tokio::task::JoinHandle;

use crate::transforms::TransformOutputsBuf;

/// Runs transform-batch futures concurrently (up to `running_limit` at a
/// time) and delivers their results in the order they were submitted,
/// regardless of completion order. Completed-but-undelivered results
/// accumulate in an internal reorder buffer bounded by `buffer_limit`.
///
/// Decoupling completion order from delivery order keeps thread utilization high
/// when one batch takes much longer than its successors: new batches can keep
/// being scheduled and run while the slow head batch finishes, and their
/// results sit in the buffer until they can be delivered in order.
///
/// # How it works
///
/// Three cursors track the lifecycle of each batch:
///
/// - `tail` — the next position to assign. Each call to `spawn` claims `tail`
///   as the new task's `pos` and pushes a `None` slot to the back of
///   `reorder_buf`, then increments `tail`.
/// - `pos` — a per-task index, assigned from `tail` at spawn time. It travels
///   with the task and is returned alongside its result so we know which slot
///   in `reorder_buf` to fill.
/// - `head` — the next position to deliver. `try_pop_ready` pops the front of
///   `reorder_buf` (only if it is `Some`) and bumps `head`, advancing the
///   delivery cursor.
///
/// `head` and `tail` are `usize` cursors that wrap on overflow: each
/// increment uses `wrapping_add(1)` and the buffer index is computed as
/// `pos.wrapping_sub(head)`. The live window (`tail.wrapping_sub(head)`) is
/// bounded by `buffer_limit`, which is far below `usize::MAX / 2`, so the
/// wrap-aware distance always equals the true distance.
///
/// Invariants:
/// - `reorder_buf.len() == tail.wrapping_sub(head)` (one slot per outstanding task)
/// - A task's result lands at `reorder_buf[pos.wrapping_sub(head)]`, which is
///   always in range because the slot was reserved as `None` in `spawn`
///
/// ## Worked example
///
/// Four tasks have been spawned (positions 0..4). Positions 0 and 1 have
/// already been delivered and popped, so `reorder_buf` only holds the
/// still-open positions {2, 3}, both initially `None`. `head = 2`, `tail = 4`.
///
/// ```text
///        delivered      still in buffer
///       |---------|    |----------------|
///   pos:  0    1         2          3
///                        ^          ^
///                       head      tail - 1
///                       (=2)       (=3)
///
///   reorder_buf:       [None]     [None]
///                       ^          ^
///                    index 0    index 1
///
///   buffer index = pos - head:
///     pos=2 → 2 - 2 = 0
///     pos=3 → 3 - 2 = 1
/// ```
///
/// Task 3 completes first (out of order). `await_drainable` writes its result
/// at `reorder_buf[3 - 2] = reorder_buf[1]`:
///
/// ```text
///   reorder_buf:       [None]    [Some(r3)]
/// ```
///
/// `try_pop_ready` returns `None` — the front is still `None`, so nothing can
/// be delivered yet.
///
/// Task 2 completes. `await_drainable` writes its result at
/// `reorder_buf[2 - 2] = reorder_buf[0]`:
///
/// ```text
///   reorder_buf:     [Some(r2)]  [Some(r3)]
/// ```
///
/// Polling the stream via `next()` now drains:
/// - pops `Some(r2)` → `head = 3`, returns r2
/// - pops `Some(r3)` → `head = 4`, returns r3
///
/// The buffer is empty (`head == tail == 4`).
pub(super) struct ConcurrentTransformScheduler {
    in_flight: FuturesUnordered<JoinHandle<(usize, TransformOutputsBuf)>>,
    reorder_buf: VecDeque<Option<TransformOutputsBuf>>,
    // head: position of the next result to deliver
    head: usize,
    // tail: position to assign to the next spawned task
    tail: usize,
    // running_limit: max number of tasks allowed to run concurrently
    running_limit: usize,
    // buffer_limit: max size of the reorder buffer — bounds memory used by
    // results waiting behind a slow head task.
    buffer_limit: usize,
}

impl ConcurrentTransformScheduler {
    pub fn new(running_limit: usize, buffer_limit: usize) -> Self {
        Self {
            in_flight: FuturesUnordered::new(),
            reorder_buf: VecDeque::with_capacity(buffer_limit),
            head: 0,
            tail: 0,
            running_limit,
            buffer_limit,
        }
    }

    /// True if a new future can be submitted right now — a CPU slot is free
    /// AND the reorder buffer has room.
    pub fn can_spawn(&self) -> bool {
        self.in_flight.len() < self.running_limit && self.reorder_buf.len() < self.buffer_limit
    }

    /// True if the head slot is already stashed — burst delivery mode after a HoL stall.
    /// Used to gate yield_now in the delivery arm: false in normal steady-state (next
    /// slot is None = task still running), so yield fires only during burst delivery.
    #[allow(dead_code)]
    pub fn has_ready(&self) -> bool {
        self.reorder_buf.front().is_some_and(Option::is_some)
    }

    /// True if no tasks are running and no results are buffered.
    pub fn is_empty(&self) -> bool {
        self.in_flight.is_empty() && self.reorder_buf.is_empty()
    }

    /// Submit a future. It is tagged with its submission position and spawned
    /// onto the current tokio runtime.
    pub fn spawn<F>(&mut self, future: F)
    where
        F: Future<Output = TransformOutputsBuf> + Send + 'static,
    {
        let pos = self.tail;
        self.tail = self.tail.wrapping_add(1);
        self.reorder_buf.push_back(None);
        self.in_flight
            .push(tokio::spawn(async move { (pos, future.await) }));
    }

    fn try_pop_ready(&mut self) -> Option<TransformOutputsBuf> {
        if !self.reorder_buf.front().is_some_and(Option::is_some) {
            return None;
        }
        self.head = self.head.wrapping_add(1);
        self.reorder_buf
            .pop_front()
            .expect("front exists: just checked above")
    }
}

impl Stream for ConcurrentTransformScheduler {
    type Item = TransformOutputsBuf;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        // Deliver buffered head — handles cascades from prior stashes.
        if let Some(buf) = this.try_pop_ready() {
            return Poll::Ready(Some(buf));
        }

        // Drain all available completions, delivering as soon as head is ready.
        while let Poll::Ready(item) = Pin::new(&mut this.in_flight).poll_next(cx) {
            match item {
                Some(Ok((pos, result))) => {
                    let idx = pos.wrapping_sub(this.head);
                    if idx == 0 {
                        // Fast path: completed task IS the head — skip stash+pop.
                        this.head = this.head.wrapping_add(1);
                        this.reorder_buf.pop_front(); // remove None placeholder
                        return Poll::Ready(Some(result));
                    }
                    this.reorder_buf[idx] = Some(result);
                    // Check if this stash unblocked the head.
                    if let Some(buf) = this.try_pop_ready() {
                        return Poll::Ready(Some(buf));
                    }
                }
                // Re-panic so the outer catch_unwind in handle_errors catches it.
                Some(Err(e)) => std::panic::resume_unwind(e.into_panic()),
                None => return Poll::Ready(None),
            }
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, time::Duration};

    use futures_util::stream::StreamExt;
    use vector_lib::event::{Event, LogEvent};

    use super::*;
    use crate::config::{DataType, TransformOutput};

    /// Build a `TransformOutputsBuf` whose primary buffer carries a single log
    /// event identifying it. Tests use this to verify delivery order: each
    /// spawned task produces a buf marked with its index, and `read_marker`
    /// extracts that index after pop.
    fn marked_buf(marker: usize) -> TransformOutputsBuf {
        let mut buf = TransformOutputsBuf::new_with_capacity(
            vec![TransformOutput::new(DataType::all_bits(), HashMap::new())],
            1,
        );
        buf.push(None, Event::Log(LogEvent::from(format!("{marker}"))));
        buf
    }

    fn read_marker(mut buf: TransformOutputsBuf) -> usize {
        let primary = buf.take_primary();
        let event = primary.into_events().next().expect("event present");
        let log = event.into_log();
        log.get_message()
            .expect("message field present")
            .to_string_lossy()
            .parse()
            .expect("marker parses as usize")
    }

    /// Drive the scheduler to completion, returning markers in delivery order.
    async fn drain(mut scheduler: ConcurrentTransformScheduler) -> Vec<usize> {
        let mut out = Vec::new();
        while let Some(buf) = scheduler.next().await {
            out.push(read_marker(buf));
        }
        out
    }

    #[tokio::test]
    async fn delivers_in_order_when_completions_in_order() {
        let mut scheduler = ConcurrentTransformScheduler::new(4, 16);
        for i in 0..10 {
            scheduler.spawn(async move { marked_buf(i) });
        }
        assert_eq!(drain(scheduler).await, (0..10).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn delivers_in_order_when_completions_out_of_order() {
        let mut scheduler = ConcurrentTransformScheduler::new(16, 16);
        // Task i sleeps (10 - i) ms — reverse order completions.
        for i in 0..10 {
            let delay = (10 - i) as u64;
            scheduler.spawn(async move {
                tokio::time::sleep(Duration::from_millis(delay)).await;
                marked_buf(i)
            });
        }
        assert_eq!(drain(scheduler).await, (0..10).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn can_spawn_respects_running_limit() {
        let mut scheduler = ConcurrentTransformScheduler::new(2, 100);
        assert!(scheduler.can_spawn());
        // Spawn two blocking tasks.
        let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);
        for _ in 0..2 {
            let tx = tx.clone();
            scheduler.spawn(async move {
                tx.send(()).await.ok();
                std::future::pending::<TransformOutputsBuf>().await
            });
        }
        // Wait for both tasks to actually start (so in_flight.len() settles).
        rx.recv().await.unwrap();
        rx.recv().await.unwrap();
        assert!(!scheduler.can_spawn(), "running_limit should block spawns");
    }

    #[tokio::test]
    async fn can_spawn_respects_buffer_limit() {
        // running_limit is very large; buffer_limit is the constraint.
        let mut scheduler = ConcurrentTransformScheduler::new(100, 3);
        for i in 0..3 {
            scheduler.spawn(async move { marked_buf(i) });
        }
        assert!(!scheduler.can_spawn(), "buffer_limit should block spawns");
    }

    #[tokio::test]
    async fn is_empty_lifecycle() {
        let mut scheduler = ConcurrentTransformScheduler::new(2, 4);
        assert!(scheduler.is_empty());

        scheduler.spawn(async move { marked_buf(7) });
        assert!(!scheduler.is_empty());

        let buf = scheduler.next().await.expect("Some");
        assert_eq!(read_marker(buf), 7);
        assert!(scheduler.is_empty());
        assert!(scheduler.next().await.is_none());
    }

    #[tokio::test]
    async fn propagates_panic() {
        let handle = tokio::spawn(async {
            let mut scheduler = ConcurrentTransformScheduler::new(2, 4);
            scheduler.spawn(async { panic!("boom") });
            scheduler.next().await;
        });
        assert!(handle.await.unwrap_err().is_panic());
    }

    #[tokio::test]
    async fn complex_out_of_order_with_running_limit() {
        // 9 tasks, running_limit = 4. Tasks complete out of order; verify that
        // the stream always delivers in spawn order.
        let mut scheduler = ConcurrentTransformScheduler::new(4, 9);

        let (txs, rxs): (Vec<_>, Vec<_>) = (0..9)
            .map(|_| tokio::sync::oneshot::channel::<()>())
            .unzip();
        let mut txs: Vec<Option<tokio::sync::oneshot::Sender<()>>> =
            txs.into_iter().map(Some).collect();
        let mut rxs: Vec<Option<tokio::sync::oneshot::Receiver<()>>> =
            rxs.into_iter().map(Some).collect();

        macro_rules! spawn_task {
            ($i:expr) => {{
                let rx = rxs[$i].take().unwrap();
                scheduler.spawn(async move {
                    rx.await.ok();
                    marked_buf($i)
                });
            }};
        }
        macro_rules! release {
            ($i:expr) => {
                txs[$i].take().unwrap().send(()).unwrap();
            };
        }
        macro_rules! next_marker {
            () => {
                read_marker(scheduler.next().await.expect("Some"))
            };
        }

        // Fill running_limit.
        spawn_task!(0);
        spawn_task!(1);
        spawn_task!(2);
        spawn_task!(3);
        assert!(!scheduler.can_spawn());

        // Tasks 2 and 1 finish out of order; head (0) still blocked.
        release!(2);
        release!(1);
        spawn_task!(4);
        spawn_task!(5);

        // Release head — cascade delivers 0, 1, 2 in order.
        release!(0);
        assert_eq!(next_marker!(), 0);
        assert_eq!(next_marker!(), 1);
        assert_eq!(next_marker!(), 2);

        // Continue — release some tail tasks while head (3) is still running.
        spawn_task!(6);
        release!(4);
        release!(5);
        spawn_task!(7);
        release!(7);
        spawn_task!(8);

        // Release head (3) — cascade delivers 3, 4, 5.
        release!(3);
        assert_eq!(next_marker!(), 3);
        assert_eq!(next_marker!(), 4);
        assert_eq!(next_marker!(), 5);

        // Release 6 (now head) — cascade delivers 6, 7.
        release!(6);
        assert_eq!(next_marker!(), 6);
        assert_eq!(next_marker!(), 7);

        release!(8);
        assert_eq!(next_marker!(), 8);

        assert!(scheduler.is_empty());
    }

    #[tokio::test]
    async fn cursors_wrap_around_usize_max() {
        // Seed both cursors near usize::MAX so that spawning more than 1 task
        // causes tail to wrap past zero, exercising:
        //   - tail.wrapping_add(1) in spawn()
        //   - pos.wrapping_sub(head) indexing in await_drainable()
        //   - head.wrapping_add(1) in try_pop_ready()
        let mut scheduler = ConcurrentTransformScheduler::new(4, 4);
        scheduler.head = usize::MAX - 1;
        scheduler.tail = usize::MAX - 1;

        // Spawn 4 tasks. Their pos values will be:
        //   usize::MAX - 1, usize::MAX, 0 (wrapped), 1
        for i in 0..4 {
            scheduler.spawn(async move { marked_buf(i) });
        }
        // After spawning: head == usize::MAX - 1, tail == 2 (wrapped).
        // tail.wrapping_sub(head) == 4 == reorder_buf.len()
        assert_eq!(
            scheduler.tail.wrapping_sub(scheduler.head),
            scheduler.reorder_buf.len(),
        );

        // Results must still be delivered in spawn order despite the wrap.
        assert_eq!(drain(scheduler).await, vec![0, 1, 2, 3]);
    }

    #[tokio::test]
    async fn stream_blocks_until_head_is_ready() {
        // Task 0 waits for a signal; task 1 completes immediately.
        // The stream must not deliver task 1 before task 0.
        let mut scheduler = ConcurrentTransformScheduler::new(4, 16);
        let (release0_tx, release0_rx) = tokio::sync::oneshot::channel::<()>();

        scheduler.spawn(async move {
            release0_rx.await.ok();
            marked_buf(0)
        });
        scheduler.spawn(async move { marked_buf(1) });

        // Release task 0; both tasks are now complete.
        release0_tx.send(()).unwrap();

        // Despite task 1 completing first internally, delivery must be 0 then 1.
        assert_eq!(read_marker(scheduler.next().await.expect("Some")), 0);
        assert_eq!(read_marker(scheduler.next().await.expect("Some")), 1);
        assert!(scheduler.next().await.is_none());
    }
}
