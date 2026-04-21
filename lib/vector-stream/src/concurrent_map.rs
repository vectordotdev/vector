use std::{
    future::Future,
    num::NonZeroUsize,
    panic,
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::{
    Stream, StreamExt,
    stream::{Fuse, FuturesOrdered},
};
use pin_project::pin_project;
use tokio::task::JoinHandle;

/// A type-erased, boxed, pinned future used solely to register a named
/// `async_backtrace` frame in the task dump while `ConcurrentMap` is parked.
type BtFrame = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

#[pin_project]
pub struct ConcurrentMap<St, T>
where
    St: Stream,
    T: Send + 'static,
{
    #[pin]
    stream: Fuse<St>,
    limit: Option<NonZeroUsize>,
    in_flight: FuturesOrdered<JoinHandle<T>>,
    f: Box<dyn Fn(St::Item) -> Pin<Box<dyn Future<Output = T> + Send + 'static>> + Send>,
    /// Holds a named async-backtrace frame while this stream is parked, so that
    /// SIGTERM task dumps show *why* ConcurrentMap is pending (upstream vs in-flight).
    bt_frame: Option<BtFrame>,
}

impl<St, T> ConcurrentMap<St, T>
where
    St: Stream,
    T: Send + 'static,
{
    pub fn new<F>(stream: St, limit: Option<NonZeroUsize>, f: F) -> Self
    where
        F: Fn(St::Item) -> Pin<Box<dyn Future<Output = T> + Send + 'static>> + Send + 'static,
    {
        Self {
            stream: stream.fuse(),
            limit,
            in_flight: FuturesOrdered::new(),
            f: Box::new(f),
            bt_frame: None,
        }
    }
}

impl<St, T> Stream for ConcurrentMap<St, T>
where
    St: Stream,
    T: Send + 'static,
{
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        // The underlying stream is done, and we have no more in-flight futures.
        if this.stream.is_done() && this.in_flight.is_empty() {
            // Clear any lingering backtrace frame on the way out.
            *this.bt_frame = None;
            return Poll::Ready(None);
        }

        loop {
            let can_poll_stream = match this.limit {
                None => true,
                Some(limit) => this.in_flight.len() < limit.get(),
            };

            if can_poll_stream {
                match this.stream.as_mut().poll_next(cx) {
                    // Even if there's no items from the underlying stream, we still have the in-flight
                    // futures to check, so we don't return just yet.
                    Poll::Pending | Poll::Ready(None) => break,
                    Poll::Ready(Some(item)) => {
                        // We received an item — clear any stale backtrace frame.
                        *this.bt_frame = None;
                        let fut = (this.f)(item);
                        let handle = tokio::spawn(fut);
                        this.in_flight.push_back(handle);
                    }
                }
            } else {
                // We're at our in-flight limit, so stop generating tasks for the moment.
                break;
            }
        }

        // Poll in-flight futures. If Pending, we're blocked waiting for spawned tasks to finish.
        // If Ready(None) with a non-empty upstream, we're blocked waiting for the upstream stream.
        match this.in_flight.poll_next_unpin(cx) {
            Poll::Pending => {
                // At least one in-flight task exists but hasn't completed yet.
                // Register (or keep alive) a named async-backtrace frame so SIGTERM
                // task dumps show "ConcurrentMap::poll_next - waiting for in-flight"
                // at the leaf, distinguishing this from the upstream-wait path.
                let frame = this.bt_frame.get_or_insert_with(|| {
                    Box::pin(async_backtrace::location!().frame(
                        std::future::pending::<()>(),
                    ))
                });
                let _ = frame.as_mut().poll(cx);
                Poll::Pending
            }
            Poll::Ready(None) if this.stream.is_done() => {
                // The stream is done, and we have no more in-flight futures.
                *this.bt_frame = None;
                Poll::Ready(None)
            }
            Poll::Ready(None) => {
                // No in-flight futures managed by FuturesOrdered, but the upstream
                // stream is not done — we must keep polling that stream.
                // Register (or keep alive) a named async-backtrace frame so SIGTERM
                // task dumps show "ConcurrentMap::poll_next - waiting for upstream"
                // at the leaf, distinguishing this from the in-flight-wait path.
                let frame = this.bt_frame.get_or_insert_with(|| {
                    Box::pin(async_backtrace::location!().frame(
                        std::future::pending::<()>(),
                    ))
                });
                let _ = frame.as_mut().poll(cx);
                Poll::Pending
            }
            Poll::Ready(Some(result)) => {
                *this.bt_frame = None;
                match result {
                    Ok(item) => Poll::Ready(Some(item)),
                    Err(e) => {
                        if let Ok(reason) = e.try_into_panic() {
                            // Resume the panic here on the calling task.
                            panic::resume_unwind(reason);
                        } else {
                            // The task was cancelled, which makes no sense, because _we_ hold the join
                            // handle. Only sensible thing to do is panic, because this is a bug.
                            panic!("concurrent map task cancelled outside of our control");
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures::channel::mpsc as futures_mpsc;
    use futures_util::stream::StreamExt;

    use super::*;

    #[tokio::test]
    async fn test_concurrent_map_on_empty_stream() {
        let stream = futures_util::stream::empty::<()>();
        let limit = Some(NonZeroUsize::new(2).unwrap());
        // The `as _` is required to construct a `dyn Future`
        let f = |()| Box::pin(async move {}) as _;
        let mut concurrent_map = ConcurrentMap::new(stream, limit, f);

        // Assert that the stream does not hang
        assert_eq!(concurrent_map.next().await, None);
    }

    /// Regression test for the at-limit waker gap.
    ///
    /// When `in_flight.len() == limit`, `ConcurrentMap::poll_next` skips polling the upstream
    /// stream entirely — it breaks out of the loop before calling `stream.poll_next(cx)`. This
    /// means `cx` is **not** registered with the upstream waker while tasks are saturating the
    /// limit. The upstream can become ready during this window without being able to wake the
    /// `ConcurrentMap` task directly.
    ///
    /// The correct behaviour is that the item eventually flows through once the at-limit tasks
    /// drain and the next `poll_next` call registers with upstream. This test verifies that
    /// no item is silently lost or causes a permanent `Pending` in this scenario.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_item_delivered_when_upstream_ready_while_at_limit() {
        // limit=1 so we reach the at-limit branch after a single spawned task.
        let limit = Some(NonZeroUsize::new(1).unwrap());

        // A channel-backed stream so we can inject items on demand.
        // futures::channel::mpsc::UnboundedReceiver implements Stream directly.
        let (upstream_tx, upstream_rx) = futures_mpsc::unbounded::<u32>();

        // A watch channel gates item 0's task: it blocks until we flip the flag to true.
        // watch::Receiver is Clone, so it can be used inside the Fn closure.
        let (hold_tx, hold_rx) = tokio::sync::watch::channel(false);

        let f = move |v: u32| {
            let mut hold_rx = hold_rx.clone();
            Box::pin(async move {
                if v == 0 {
                    // Wait until the test releases us, keeping in_flight saturated at limit=1.
                    hold_rx.wait_for(|&released| released).await.unwrap();
                }
                v
            }) as _
        };

        // Collect outputs in a background task so we can interleave sends.
        let (result_tx, mut result_rx) = futures_mpsc::unbounded::<u32>();
        let collect = tokio::spawn(async move {
            let mut map = ConcurrentMap::new(upstream_rx, limit, f);
            while let Some(v) = map.next().await {
                result_tx.unbounded_send(v).unwrap();
            }
        });

        // Send item 0 — spawns a task that blocks immediately, filling in_flight to the limit.
        upstream_tx.unbounded_send(0).unwrap();

        // Yield so the spawned map task runs and parks on hold_rx.
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Send item 1 while in_flight is at its limit. ConcurrentMap has NOT polled upstream
        // since hitting the limit, so cx is not registered there. Item 1 waits in the channel.
        upstream_tx.unbounded_send(1).unwrap();

        // Close the upstream so the stream ends after item 1.
        drop(upstream_tx);

        // Release item 0's task. ConcurrentMap should then drain, poll upstream, get item 1.
        hold_tx.send(true).unwrap();

        // Both items must arrive within a generous timeout.
        let r0 = tokio::time::timeout(Duration::from_secs(5), result_rx.next())
            .await
            .expect("timed out waiting for item 0")
            .expect("channel closed");
        let r1 = tokio::time::timeout(Duration::from_secs(5), result_rx.next())
            .await
            .expect("ConcurrentMap hung — item 1 not delivered after at-limit task completed")
            .expect("channel closed");

        assert_eq!((r0, r1), (0, 1));
        collect.await.unwrap();
    }

    /// Tests that an item arriving in the upstream while ConcurrentMap has zero in-flight
    /// tasks (the `None => Poll::Pending` arm) is eventually delivered without hanging.
    ///
    /// This exercises the code path where `FuturesOrdered::poll_next_unpin` returns
    /// `Ready(None)` synchronously (empty queue), `ready!` fires immediately, and
    /// `Poll::Pending` is returned. The only wake path is via the upstream's registered `cx`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_item_delivered_after_pending_with_empty_in_flight() {
        let limit = Some(NonZeroUsize::new(4).unwrap());

        let (tx, rx) = futures_mpsc::unbounded::<u32>();

        let f = |v: u32| Box::pin(async move { v }) as _;

        // Collect in background so we can send after the first Pending.
        let (result_tx, mut result_rx) = futures_mpsc::unbounded::<u32>();
        let collect = tokio::spawn(async move {
            let mut map = ConcurrentMap::new(rx, limit, f);
            while let Some(v) = map.next().await {
                result_tx.unbounded_send(v).unwrap();
            }
        });

        // Yield to let the map task start and reach Pending on the empty channel.
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Send an item. The upstream channel's waker (registered when map got Pending)
        // must fire and deliver the item.
        tx.unbounded_send(42).unwrap();
        drop(tx);

        let result = tokio::time::timeout(Duration::from_secs(5), result_rx.next())
            .await
            .expect("ConcurrentMap hung after upstream became ready following empty-in-flight Pending")
            .expect("channel closed");

        assert_eq!(result, 42);
        collect.await.unwrap();
    }
}
