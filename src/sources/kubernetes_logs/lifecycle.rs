use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{
    channel::oneshot,
    future::{select, BoxFuture, Either},
    pin_mut, ready,
    stream::{FuturesUnordered, FuturesOrdered},
    FutureExt, StreamExt,
};

use crate::shutdown::{ShutdownSignal, ShutdownSignalToken};

#[derive(Debug)]
pub enum LifecycleOrder{
    Ordered,
    Unordered,
}

/// Lifecycle encapsulates logic for managing a lifecycle of multiple futures
/// that are bounded together by a shared shutdown condition.
///
/// If any of the futures completes, or global shutdown it requested, all of the
/// managed futures are requested to shutdown. They can do so gracefully after
/// completing their work.
#[derive(Debug)]
pub struct Lifecycle<'bound> {
    futs_ordered: FuturesOrdered<BoxFuture<'bound, ()>>,
    futs_unordered: FuturesUnordered<BoxFuture<'bound, ()>>,
    order: LifecycleOrder,
    fut_shutdowns: Vec<oneshot::Sender<()>>,
}

/// Holds a "global" shutdown signal or shutdown signal token.
/// Effectively used to hold the token or signal such that it can be dropped
/// after the shutdown is complete.
#[derive(Debug)]
pub enum GlobalShutdownToken {
    /// The global shutdown signal was consumed, and we have a raw
    /// [`ShutdownSignalToken`] now.
    Token(ShutdownSignalToken),
    /// The [`ShutdownSignal`] wasn't consumed, and still holds on to the
    /// [`ShutdownSignalToken`]. Keep it around.
    Unused(ShutdownSignal),
}

impl<'bound> Lifecycle<'bound> {
    /// Create a new [`Lifecycle`].
    pub fn new(lc_order: LifecycleOrder) -> Self {
        return Self {
            futs_ordered: FuturesOrdered::new(),
            futs_unordered: FuturesUnordered::new(),
            order: lc_order,
            fut_shutdowns: Vec::new(),
        }
    }

    /// Add a new future to be managed by the [`Lifecycle`].
    ///
    /// Returns a [`Slot`] to be bound with the `Future`, and
    /// a [`ShutdownHandle`] that is to be used by the bound future to wait for
    /// shutdown.
    pub fn add(&mut self) -> (Slot<'bound, '_>, ShutdownHandle) {
        let (tx, rx) = oneshot::channel();
        let slot = Slot {
            lifecycle: self,
            shutdown_trigger: tx,
        };
        let shutdown_handle = ShutdownHandle(rx);
        (slot, shutdown_handle)
    }

    /// Run the managed futures and keep track of the shutdown process.
    pub async fn run(mut self, mut global_shutdown: ShutdownSignal) -> GlobalShutdownToken {
        let token;
        match self.order {
            LifecycleOrder::Ordered => {
                let first_task_fut = self.futs_ordered.next();

                pin_mut!(first_task_fut);

                token = match select(first_task_fut, &mut global_shutdown).await {
                    Either::Left((None, _)) => {
                        trace!(message = "Lifecycle had no tasks upon run, we're done.");
                        GlobalShutdownToken::Unused(global_shutdown)
                    }
                    Either::Left((Some(()), _)) => {
                        trace!(message = "Lifecycle had the first task completed.");
                        GlobalShutdownToken::Unused(global_shutdown)
                    }
                    Either::Right((shutdown_signal_token, _)) => {
                        trace!(message = "Lifecycle got a global shutdown request.");
                        GlobalShutdownToken::Token(shutdown_signal_token)
                    }
                };
            },
            LifecycleOrder::Unordered => {
                let first_task_fut = self.futs_unordered.next();

                pin_mut!(first_task_fut);

                token = match select(first_task_fut, &mut global_shutdown).await {
                    Either::Left((None, _)) => {
                        trace!(message = "Lifecycle had no tasks upon run, we're done.");
                        GlobalShutdownToken::Unused(global_shutdown)
                    }
                    Either::Left((Some(()), _)) => {
                        trace!(message = "Lifecycle had the first task completed.");
                        GlobalShutdownToken::Unused(global_shutdown)
                    }
                    Either::Right((shutdown_signal_token, _)) => {
                        trace!(message = "Lifecycle got a global shutdown request.");
                        GlobalShutdownToken::Token(shutdown_signal_token)
                    }
                };
            }
        }

        // Send the shutdowns to all managed futures.
        for fut_shutdown in self.fut_shutdowns {
            if fut_shutdown.send(()).is_err() {
                trace!(
                    message = "Error while sending a future shutdown, \
                        the receiver is already dropped; \
                        this is not a problem."
                );
            }
        }

        match self.order {
            LifecycleOrder::Ordered => {
             // Wait for all the futures to complete.
                while let Some(()) = self.futs_ordered.next().await {
                    trace!(message = "A lifecycle-managed future completed after shutdown was requested.");
                }
            },
            LifecycleOrder::Unordered => {
                // Wait for all the futures to complete.
                while let Some(()) = self.futs_unordered.next().await {
                    trace!(message = "A lifecycle-managed future completed after shutdown was requested.");
                }
            }
        }

        // Return the global shutdown token so that caller can perform it's
        // cleanup.
        token
    }
}

/// Represents an unbounded slot at the lifecycle.
#[derive(Debug)]
pub struct Slot<'bound, 'lc> {
    lifecycle: &'lc mut Lifecycle<'bound>,
    shutdown_trigger: oneshot::Sender<()>,
}

impl<'bound, 'lc> Slot<'bound, 'lc> {
    /// Bind the lifecycle slot to a concrete future.
    /// The passed future MUST start it's shutdown process when requested to
    /// shutdown via the signal passed from the corresponding
    /// [`ShutdownHandle`].
    pub fn bind(self, future: BoxFuture<'bound, ()>) {
        match self.lifecycle.order {
            LifecycleOrder::Ordered => self.lifecycle.futs_ordered.push(future),
            LifecycleOrder::Unordered => self.lifecycle.futs_unordered.push(future),
        }
        // self.lifecycle.futs.push(future);
        self.lifecycle.fut_shutdowns.push(self.shutdown_trigger);
    }
}

/// A handle that allows waiting for the lifecycle-issued shutdown.
#[derive(Debug)]
pub struct ShutdownHandle(oneshot::Receiver<()>);

impl Future for ShutdownHandle {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let _ = ready!(self.0.poll_unpin(cx));
        Poll::Ready(())
    }
}
