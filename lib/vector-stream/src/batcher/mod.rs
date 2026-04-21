pub mod config;
pub mod data;
pub mod limiter;

use std::{
    pin::Pin,
    task::{Context, Poll},
};

pub use config::BatchConfig;
use futures::{
    Future, StreamExt,
    stream::{Fuse, Stream},
};
use pin_project::pin_project;
use tokio::time::Sleep;

/// A type-erased, boxed, pinned future used solely to register a named
/// `async_backtrace` frame in the task dump while `Batcher` is parked.
type BtFrame = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

#[pin_project]
pub struct Batcher<S, C> {
    state: C,

    #[pin]
    /// The stream this `Batcher` wraps
    stream: Fuse<S>,

    #[pin]
    timer: Maybe<Sleep>,

    /// Holds a named async-backtrace frame while this batcher is parked, so that
    /// SIGTERM task dumps show *why* Batcher is pending (upstream vs timer).
    bt_frame: Option<BtFrame>,
}

/// An `Option`, but with pin projection
#[pin_project(project = MaybeProj)]
pub enum Maybe<T> {
    Some(#[pin] T),
    None,
}

impl<S, C> Batcher<S, C>
where
    S: Stream,
    C: BatchConfig<S::Item>,
{
    pub fn new(stream: S, config: C) -> Self {
        Self {
            state: config,
            stream: stream.fuse(),
            timer: Maybe::None,
            bt_frame: None,
        }
    }
}

impl<S, C> Stream for Batcher<S, C>
where
    S: Stream,
    C: BatchConfig<S::Item>,
{
    type Item = C::Batch;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            let mut this = self.as_mut().project();
            match this.stream.poll_next(cx) {
                Poll::Ready(None) => {
                    *this.bt_frame = None;
                    return {
                        if this.state.len() == 0 {
                            Poll::Ready(None)
                        } else {
                            Poll::Ready(Some(this.state.take_batch()))
                        }
                    };
                }
                Poll::Ready(Some(item)) => {
                    *this.bt_frame = None;
                    let (item_fits, item_metadata) = this.state.item_fits_in_batch(&item);
                    if item_fits {
                        this.state.push(item, item_metadata);
                        if this.state.is_batch_full() {
                            this.timer.set(Maybe::None);
                            return Poll::Ready(Some(this.state.take_batch()));
                        } else if this.state.len() == 1 {
                            this.timer
                                .set(Maybe::Some(tokio::time::sleep(this.state.timeout())));
                        }
                    } else {
                        let output = Poll::Ready(Some(this.state.take_batch()));
                        this.state.push(item, item_metadata);
                        this.timer
                            .set(Maybe::Some(tokio::time::sleep(this.state.timeout())));
                        return output;
                    }
                }
                Poll::Pending => {
                    return {
                        if let MaybeProj::Some(timer) = this.timer.as_mut().project() {
                            // Upstream has no new items; check if the flush timer has fired.
                            match timer.poll(cx) {
                                Poll::Ready(()) => {
                                    // Timer fired — flush the current batch.
                                    *this.bt_frame = None;
                                    this.timer.set(Maybe::None);
                                    debug_assert!(
                                        this.state.len() != 0,
                                        "timer should have been cancelled"
                                    );
                                    Poll::Ready(Some(this.state.take_batch()))
                                }
                                Poll::Pending => {
                                    // Both upstream and the flush timer are pending.
                                    // Register (or keep alive) a named async-backtrace frame so
                                    // SIGTERM task dumps show "Batcher::poll_next - waiting for timer"
                                    // at the leaf, distinguishing this from the no-timer path.
                                    let frame = this.bt_frame.get_or_insert_with(|| {
                                        Box::pin(async_backtrace::location!().frame(
                                            std::future::pending::<()>(),
                                        ))
                                    });
                                    let _ = frame.as_mut().poll(cx);
                                    Poll::Pending
                                }
                            }
                        } else {
                            // No timer active and upstream is not ready — we're parked
                            // waiting for upstream to produce the first item of a new batch.
                            // Register (or keep alive) a named async-backtrace frame so SIGTERM
                            // task dumps show "Batcher::poll_next - waiting for upstream"
                            // at the leaf, distinguishing this from the timer-wait path.
                            let frame = this.bt_frame.get_or_insert_with(|| {
                                Box::pin(async_backtrace::location!().frame(
                                    std::future::pending::<()>(),
                                ))
                            });
                            let _ = frame.as_mut().poll(cx);
                            Poll::Pending
                        }
                    };
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}

#[cfg(test)]
#[allow(clippy::similar_names)]
mod test {
    use std::{num::NonZeroUsize, time::Duration};

    use futures::{channel::mpsc as futures_mpsc, stream};

    use super::*;
    use crate::BatcherSettings;

    /// After a timer-driven flush the `Batcher` must deliver a second wave of events that
    /// arrived in the upstream channel during the dormant period (no poll calls).
    ///
    /// This test isolates the sequence that matches the observed production stall:
    ///
    /// 1. A batch is assembled and flushed by the timer (not size limit), leaving
    ///    timer=None and batch empty.
    /// 2. The Batcher is NOT polled for a period (simulating the driver busy in arm 1).
    /// 3. New events arrive in the upstream channel during that dormant window.
    /// 4. The Batcher is polled again (arm 3 re-entry after all in-flight drain).
    /// 5. Events must flow — the Batcher must NOT permanently hang.
    ///
    /// In step 4, the channel buffer already has the items so `stream.poll_next` returns
    /// `Ready(Some(...))` synchronously — the channel waker path is NOT required for the
    /// immediate delivery. A second `poll` after the item is consumed triggers the timer
    /// path; we advance time to fire the timer.
    ///
    /// If the Batcher's waker machinery is broken in this transition, it will park on
    /// `Poll::Pending` and never wake even after the timer elapses.
    #[tokio::test(start_paused = true)]
    async fn batch_delivered_after_timer_flush_and_dormant_period() {
        let timeout = Duration::from_millis(100);

        // Use an unbounded channel so we can inject events on demand.
        let (tx, rx) = futures_mpsc::unbounded::<u32>();

        let settings = BatcherSettings::new(
            timeout,
            NonZeroUsize::new(10_000).unwrap(), // large size limit — only timer flushes
            NonZeroUsize::new(10_000).unwrap(),
        );
        let batcher = Batcher::new(rx, settings.as_item_size_config(|x: &u32| *x as usize));
        tokio::pin!(batcher);

        // --- Phase 1: assemble first batch and flush via timer ---

        tx.unbounded_send(1u32).unwrap();
        {
            let mut next = batcher.next();
            assert_eq!(futures::poll!(&mut next), Poll::Pending);
            tokio::time::advance(timeout * 2).await;
            let batch1 = next.await.expect("first batch should arrive");
            assert_eq!(batch1, vec![1u32]);
        }
        // State: timer=None, batch=empty, channel=empty.

        // --- Phase 2: simulate dormancy — events arrive without any poll ---
        // This mirrors the driver being in arm 1 (completing in-flight requests) while
        // Kafka continues to push events into the channel.
        tx.unbounded_send(2u32).unwrap();

        // --- Phase 3: arm-3 re-entry — poll batcher to retrieve the second batch ---
        // The channel already has event 2 buffered, so the first poll consumes it
        // synchronously. The Batcher then sets a new timer and parks.
        {
            let mut next2 = batcher.next();

            // First poll: channel has buffered item 2 → consumed, timer set, channel now
            // empty → parks on timer → Pending.
            assert_eq!(
                futures::poll!(&mut next2),
                Poll::Pending,
                "expected Pending while waiting for timer after consuming buffered event"
            );

            // Advance time past the new timer.
            tokio::time::advance(timeout * 2).await;

            // Timer fires — batch containing event 2 must be delivered.
            let batch2 = next2
                .await
                .expect("Batcher hung after dormancy period — timer waker lost");
            assert_eq!(batch2, vec![2u32]);
        }
    }

    /// Verifies that the Batcher properly registers the upstream channel's waker even when
    /// the channel is empty at poll time (the "no timer, no items" parking state).
    ///
    /// This is the state the Batcher is in after a timer flush when no new events have
    /// arrived yet. If the waker is dropped in this state, any subsequent upstream event
    /// will never wake the driver's arm 3.
    ///
    /// Sequence exercised:
    ///   1. Timer flush: batch assembled and flushed, leaving timer=None, batch=empty.
    ///   2. Batcher polled with empty channel → Pending, channel waker registered.
    ///   3. Event sent to channel while `next` future is live.
    ///   4. Time advanced past new timer → `next.await` completes with second batch.
    ///
    /// If step 2 does NOT register the channel waker, the event in step 3 is never seen
    /// and the final `next.await` hangs forever (failing the test via timeout).
    #[tokio::test(start_paused = true)]
    async fn upstream_waker_registered_when_timer_none_and_batch_empty() {
        let timeout = Duration::from_millis(100);

        let (tx, rx) = futures_mpsc::unbounded::<u32>();

        let settings = BatcherSettings::new(
            timeout,
            NonZeroUsize::new(10_000).unwrap(),
            NonZeroUsize::new(10_000).unwrap(),
        );
        let batcher = Batcher::new(rx, settings.as_item_size_config(|x: &u32| *x as usize));
        tokio::pin!(batcher);

        // Step 1: assemble and flush via timer.
        tx.unbounded_send(10u32).unwrap();
        {
            // Keep the same future alive across poll + advance so the timer wakes it.
            let mut next = batcher.next();
            assert_eq!(futures::poll!(&mut next), Poll::Pending);
            tokio::time::advance(timeout * 2).await;
            let b = next.await.unwrap();
            assert_eq!(b, vec![10u32]);
        }
        // Batcher state: timer=None, batch=empty.

        // Step 2: park with empty channel to register the channel waker, then send an
        // event and advance time — all while keeping the *same* future alive so it can
        // be woken by the channel and then by the new timer.
        {
            let mut next2 = batcher.next();

            // First poll: channel empty → Pending, channel waker registered for this task.
            assert_eq!(
                futures::poll!(&mut next2),
                Poll::Pending,
                "expected Pending when channel empty and no timer"
            );

            // Step 3: inject an event. The channel waker (registered above) marks our task
            // ready. The item waits in the channel buffer — it will be consumed on the next
            // poll of `next2`.
            tx.unbounded_send(20u32).unwrap();

            // At this point, if we poll `next2` again the Batcher will consume event 20,
            // push it, set a new timer (100 ms), then park on that timer.
            assert_eq!(
                futures::poll!(&mut next2),
                Poll::Pending,
                "expected Pending while waiting for new timer after consuming event 20"
            );

            // Step 4: advance time past the new timer.
            tokio::time::advance(timeout * 2).await;

            // The timer fires and the Batcher flushes event 20.
            let b2 = next2
                .await
                .expect("Batcher hung — upstream waker lost in timer=None batch=empty state");
            assert_eq!(b2, vec![20u32]);
        }
    }

    /// Regression test: after a timer flush, a second wave of events must be delivered
    /// even if the Batcher was NOT polled between the flush and the new events arriving.
    ///
    /// Specifically tests the "at-limit gap" scenario:
    ///
    /// - ConcurrentMap is at its concurrency limit → upstream (Batcher) is not polled
    /// - During that gap the Batcher's timer fires and flushes one batch
    /// - Then in-flight tasks complete → ConcurrentMap is no longer at limit
    /// - ConcurrentMap polls Batcher → second batch must flow (not hang)
    ///
    /// Uses a manual poll via `futures::poll!` to control timing precisely.
    #[tokio::test(start_paused = true)]
    async fn second_batch_delivered_after_timer_flush_during_at_limit_gap() {
        let timeout = Duration::from_millis(100);
        let (tx, rx) = futures_mpsc::unbounded::<u32>();

        let settings = BatcherSettings::new(
            timeout,
            NonZeroUsize::new(10_000).unwrap(),
            NonZeroUsize::new(10_000).unwrap(),
        );
        let batcher = Batcher::new(rx, settings.as_item_size_config(|x: &u32| *x as usize));
        tokio::pin!(batcher);

        // Send first event and park waiting for timer.
        tx.unbounded_send(1u32).unwrap();
        assert_eq!(futures::poll!(batcher.next()), Poll::Pending);

        // Advance past the timeout — timer fires.
        tokio::time::advance(timeout * 2).await;

        // Simulate the ConcurrentMap "at limit" gap: poll the batcher to consume the
        // timer-flushed batch, then DO NOT poll again for a while.
        let batch1 = batcher.next().await.unwrap();
        assert_eq!(batch1, vec![1u32]);

        // Events arrive during the "gap" (ConcurrentMap at limit, no poll).
        tx.unbounded_send(2u32).unwrap();
        tx.unbounded_send(3u32).unwrap();

        // Simulate ConcurrentMap re-entering after all in-flight drain:
        // poll batcher to get the second batch. The items are immediately in the channel
        // buffer so poll should return Ready without needing the timer yet.
        let poll_result = futures::poll!(batcher.next());
        // The batcher may have consumed event 2 synchronously (channel had data).
        // If it returns Pending here, it must eventually deliver via timer or waker.
        let batch2 = match poll_result {
            Poll::Ready(Some(b)) => {
                // Delivered synchronously — great.
                b
            }
            Poll::Pending => {
                // The batcher parked — advance time to trigger the timer.
                tokio::time::advance(timeout * 2).await;
                tokio::time::timeout(Duration::from_millis(500), batcher.next())
                    .await
                    .expect("Batcher hung after second wave of events")
                    .expect("stream ended")
            }
            Poll::Ready(None) => panic!("stream ended prematurely"),
        };

        assert!(
            !batch2.is_empty(),
            "second batch should contain at least one event"
        );
    }

    #[tokio::test]
    async fn item_limit() {
        let stream = stream::iter([1, 2, 3]);
        let settings = BatcherSettings::new(
            Duration::from_millis(100),
            NonZeroUsize::new(10000).unwrap(),
            NonZeroUsize::new(2).unwrap(),
        );
        let batcher = Batcher::new(stream, settings.as_item_size_config(|x: &u32| *x as usize));
        let batches: Vec<_> = batcher.collect().await;
        assert_eq!(batches, vec![vec![1, 2], vec![3],]);
    }

    #[tokio::test]
    async fn size_limit() {
        let batcher = Batcher::new(
            stream::iter([1, 2, 3, 4, 5, 6, 2, 3, 1]),
            BatcherSettings::new(
                Duration::from_millis(100),
                NonZeroUsize::new(5).unwrap(),
                NonZeroUsize::new(100).unwrap(),
            )
            .as_item_size_config(|x: &u32| *x as usize),
        );
        let batches: Vec<_> = batcher.collect().await;
        assert_eq!(
            batches,
            vec![
                vec![1, 2],
                vec![3],
                vec![4],
                vec![5],
                vec![6],
                vec![2, 3],
                vec![1],
            ]
        );
    }

    #[tokio::test]
    async fn timeout_limit() {
        tokio::time::pause();

        let timeout = Duration::from_millis(100);
        let stream = stream::iter([1, 2]).chain(stream::pending());
        let batcher = Batcher::new(
            stream,
            BatcherSettings::new(
                timeout,
                NonZeroUsize::new(5).unwrap(),
                NonZeroUsize::new(100).unwrap(),
            )
            .as_item_size_config(|x: &u32| *x as usize),
        );

        tokio::pin!(batcher);
        let mut next = batcher.next();
        assert_eq!(futures::poll!(&mut next), Poll::Pending);
        tokio::time::advance(timeout).await;
        let batch = next.await;
        assert_eq!(batch, Some(vec![1, 2]));
    }
}
