use std::{collections::VecDeque, future::Future};

use futures_util::stream::{FuturesUnordered, StreamExt};
use tokio::task::{JoinError, JoinHandle};

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
/// A `while let Some(r) = scheduler.try_pop_ready()` loop now drains:
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
            reorder_buf: VecDeque::new(),
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

    /// True if any task is still running. Used to gate the `await_drainable`
    /// arm in `tokio::select!` since `await_drainable` on an empty scheduler
    /// would hang
    pub fn has_running(&self) -> bool {
        !self.in_flight.is_empty()
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

    /// Wait for any task to complete and stash its result in the reorder
    /// buffer. After this resolves, one or more results may be available via
    /// `try_pop_ready`. Returns `Err` if a task panicked.
    pub async fn await_drainable(&mut self) -> Result<(), JoinError> {
        match self.in_flight.next().await {
            Some(Ok((pos, result))) => {
                // pos.wrapping_sub(head) is the index into reorder_buf. The
                // entry was reserved as None in spawn, and we use wrapping_sub
                // so it is always in bounds / a valid integer.
                self.reorder_buf[pos.wrapping_sub(self.head)] = Some(result);
                Ok(())
            }
            Some(Err(join_err)) => Err(join_err),
            None => unreachable!("await_drainable called with no tasks in flight"),
        }
    }

    /// Pop the next consecutive ready result. Returns `None` if the head slot
    /// is still running or the buffer is empty. Non-blocking.
    pub fn try_pop_ready(&mut self) -> Option<TransformOutputsBuf> {
        if !self.reorder_buf.front().is_some_and(Option::is_some) {
            return None;
        }
        self.head = self.head.wrapping_add(1);
        self.reorder_buf
            .pop_front()
            .expect("front exists: just checked above")
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, time::Duration};

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
        while scheduler.has_running() {
            scheduler.await_drainable().await.unwrap();
            while let Some(buf) = scheduler.try_pop_ready() {
                out.push(read_marker(buf));
            }
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
        assert!(!scheduler.has_running());

        scheduler.spawn(async move { marked_buf(7) });
        assert!(!scheduler.is_empty());
        assert!(scheduler.has_running());

        scheduler.await_drainable().await.unwrap();
        // Still not empty: result is buffered, hasn't been popped.
        assert!(!scheduler.is_empty());
        assert!(!scheduler.has_running());

        let buf = scheduler.try_pop_ready().expect("ready");
        assert_eq!(read_marker(buf), 7);
        assert!(scheduler.is_empty());
    }

    #[tokio::test]
    async fn await_drainable_propagates_panic() {
        let mut scheduler = ConcurrentTransformScheduler::new(2, 4);
        scheduler.spawn(async { panic!("boom") });
        let err = scheduler.await_drainable().await.unwrap_err();
        assert!(err.is_panic());
    }

    #[tokio::test]
    async fn complex_out_of_order_with_running_limit() {
        // 9 tasks, running_limit = 4. Each task awaits its own oneshot before
        // returning its position, so the test deterministically chooses the
        // completion order by releasing channels in a specific sequence.
        // Tracks delivery throughout to verify head-of-line cascades only fire
        // when the head slot becomes ready.
        let mut scheduler = ConcurrentTransformScheduler::new(4, 9);

        let (txs, rxs): (Vec<_>, Vec<_>) = (0..9)
            .map(|_| tokio::sync::oneshot::channel::<()>())
            .unzip();
        let mut txs: Vec<Option<tokio::sync::oneshot::Sender<()>>> =
            txs.into_iter().map(Some).collect();
        let mut rxs: Vec<Option<tokio::sync::oneshot::Receiver<()>>> =
            rxs.into_iter().map(Some).collect();
        let mut delivered: Vec<usize> = Vec::new();

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
        macro_rules! recv_and_drain {
            () => {{
                scheduler.await_drainable().await.unwrap();
                while let Some(buf) = scheduler.try_pop_ready() {
                    delivered.push(read_marker(buf));
                }
            }};
        }

        // Spawn 4 — running_limit reached.
        spawn_task!(0);
        spawn_task!(1);
        spawn_task!(2);
        spawn_task!(3);
        assert!(!scheduler.can_spawn());

        // Task 2 finishes first — head (0) still running, no delivery.
        release!(2);
        recv_and_drain!();
        assert!(delivered.is_empty());

        // Task 1 finishes — still no delivery.
        release!(1);
        recv_and_drain!();
        assert!(delivered.is_empty());

        // in_flight has {0, 3}; refill with 4 and 5.
        spawn_task!(4);
        spawn_task!(5);
        assert!(!scheduler.can_spawn());

        // Head (0) finishes — cascade drains 0, 1, 2.
        release!(0);
        recv_and_drain!();
        assert_eq!(delivered, vec![0, 1, 2]);

        // in_flight has {3, 4, 5}; spawn 6.
        spawn_task!(6);
        assert!(!scheduler.can_spawn());

        // Task 4 finishes — head (3) still running.
        release!(4);
        recv_and_drain!();
        assert_eq!(delivered, vec![0, 1, 2]);

        spawn_task!(7);
        assert!(!scheduler.can_spawn());

        // Task 5 finishes — head (3) still running.
        release!(5);
        recv_and_drain!();
        assert_eq!(delivered, vec![0, 1, 2]);

        spawn_task!(8);
        assert!(!scheduler.can_spawn());

        // Task 7 finishes — head (3) still running.
        release!(7);
        recv_and_drain!();
        assert_eq!(delivered, vec![0, 1, 2]);

        // Release the head (3) — cascade drains 3, 4, 5.
        release!(3);
        recv_and_drain!();
        assert_eq!(delivered, vec![0, 1, 2, 3, 4, 5]);

        // Release 6 (now head) — cascade drains 6, 7.
        release!(6);
        recv_and_drain!();
        assert_eq!(delivered, vec![0, 1, 2, 3, 4, 5, 6, 7]);

        // Release 8.
        release!(8);
        recv_and_drain!();
        assert_eq!(delivered, vec![0, 1, 2, 3, 4, 5, 6, 7, 8]);

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
    async fn try_pop_ready_returns_none_when_head_not_ready() {
        let mut scheduler = ConcurrentTransformScheduler::new(4, 16);
        let (release0_tx, release0_rx) = tokio::sync::oneshot::channel::<()>();

        // Task 0 waits for a signal; task 1 completes immediately.
        scheduler.spawn(async move {
            release0_rx.await.ok();
            marked_buf(0)
        });
        scheduler.spawn(async move { marked_buf(1) });

        // Wait for task 1 to finish and be stashed.
        scheduler.await_drainable().await.unwrap();
        // Head (task 0) is still running — nothing ready to pop in order.
        assert!(scheduler.try_pop_ready().is_none());

        // Release task 0.
        release0_tx.send(()).unwrap();
        scheduler.await_drainable().await.unwrap();
        let b0 = scheduler.try_pop_ready().expect("0 ready");
        let b1 = scheduler.try_pop_ready().expect("1 ready");
        assert_eq!(read_marker(b0), 0);
        assert_eq!(read_marker(b1), 1);
        assert!(scheduler.try_pop_ready().is_none());
    }
}
