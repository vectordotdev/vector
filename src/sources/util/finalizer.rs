use std::marker::{PhantomData, Unpin};
use std::{future::Future, pin::Pin, task::Context, task::Poll};

use futures::stream::{FuturesOrdered, FuturesUnordered};
use futures::{FutureExt, Stream, StreamExt};
use stream_cancel::{Trigger, Tripwire};
use tokio::sync::mpsc;

use crate::event::{BatchStatus, BatchStatusReceiver};
use crate::shutdown::ShutdownSignal;

/// The `OrderedFinalizer` framework marks events from a source as
/// done in a single background task *in the order they are received
/// from the source*, using `FinalizerSet`.
#[cfg(any(
    feature = "sources-file",
    feature = "sources-journald",
    feature = "sources-kafka",
))]
pub(crate) type OrderedFinalizer<T> = FinalizerSet<T, FuturesOrdered<FinalizerFuture<T>>, true>;

/// The `UnorderedFinalizer` framework marks events from a source as
/// done in a single background task *in the order the finalization
/// happens on the event batches*, using `FinalizerSet`.
#[cfg(any(
    feature = "sources-aws_sqs",
    feature = "sources-splunk_hec",
    feature = "sources-gcp_pubsub"
))]
pub(crate) type UnorderedFinalizer<T> =
    FinalizerSet<T, FuturesUnordered<FinalizerFuture<T>>, false>;

/// The `FinalizerSet` framework here is a mechanism for marking
/// events from a source as done in a single background task. The type
/// `T` is the source-specific data associated with each entry to be
/// used to complete the finalization.
#[pin_project::pin_project]
pub(crate) struct FinalizerSet<T, S, const SOE: bool> {
    sender: Option<mpsc::UnboundedSender<(BatchStatusReceiver, T)>>,
    failed: Option<Tripwire>,
    _phantom: PhantomData<S>,
}

impl<T, S, const SOE: bool> FinalizerSet<T, S, SOE>
where
    T: Send + 'static,
    S: FuturesSet<FinalizerFuture<T>> + Default + Send + Unpin + 'static,
{
    pub(crate) fn new<F, Fut>(shutdown: ShutdownSignal, apply_done: F) -> Self
    where
        F: Fn(T) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let (sender, receiver) = mpsc::unbounded_channel();
        let (trigger, tripwire) = SOE
            .then(Tripwire::new)
            .map_or((None, None), |(trig, trip)| (Some(trig), Some(trip)));
        tokio::spawn(
            Runner {
                shutdown,
                new_entries: receiver,
                apply_done,
                status_receivers: S::default(),
                failure: trigger,
            }
            .run::<SOE>(),
        );
        Self {
            sender: Some(sender),
            failed: tripwire,
            _phantom: Default::default(),
        }
    }

    pub(crate) fn add(&self, entry: T, receiver: BatchStatusReceiver) {
        if let Some(sender) = &self.sender {
            if let Err(error) = sender.send((receiver, entry)) {
                error!(message = "FinalizerSet task ended prematurely.", %error);
            }
        }
    }

    pub fn failure_future(&mut self) -> Option<Tripwire> {
        self.failed.take()
    }
}

impl<T> Future for OrderedFinalizer<T> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        match this.failed {
            None => Poll::Pending, // Never returns if there is no failure trigger
            Some(failed) => match failed.poll_unpin(ctx) {
                Poll::Ready(true) => Poll::Ready(()),
                Poll::Ready(false) => {
                    this.failed.take();
                    Poll::Pending
                }
                Poll::Pending => Poll::Pending,
            },
        }
    }
}

struct Runner<T, S, F> {
    shutdown: ShutdownSignal,
    new_entries: mpsc::UnboundedReceiver<(BatchStatusReceiver, T)>,
    apply_done: F,
    status_receivers: S,
    failure: Option<Trigger>,
}

impl<T, S, F, Fut> Runner<T, S, F>
where
    S: FuturesSet<FinalizerFuture<T>> + Unpin,
    F: Fn(T) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    async fn run<const SOE: bool>(mut self) {
        loop {
            tokio::select! {
                _ = self.shutdown.clone() => break,
                new_entry = self.new_entries.recv() => match new_entry {
                    Some((receiver, entry)) => {
                        self.status_receivers.push(FinalizerFuture {
                            receiver,
                            entry: Some(entry),
                        });
                    }
                    None => break,
                },
                finished = self.status_receivers.next(), if !self.status_receivers.is_empty() => match finished {
                    Some((status, entry)) => if status == BatchStatus::Delivered {
                        (self.apply_done)(entry).await
                    } else if SOE {
                        return
                    }
                    // The is_empty guard above prevents this from being reachable.
                    None => unreachable!(),
                },
            }
        }
        // We've either seen a shutdown signal or the new entry sender was
        // closed. Wait for the last statuses to come in before indicating
        // we are done.
        while let Some((status, entry)) = self.status_receivers.next().await {
            if status == BatchStatus::Delivered {
                (self.apply_done)(entry).await;
            } else if SOE {
                return;
            }
        }
        // Note: `shutdown` is automatically dropped on return from this
        // function, signalling this component is completed.
        if let Some(failure) = self.failure {
            failure.cancel();
        }
    }
}

pub(crate) trait FuturesSet<Fut: Future>: Stream<Item = Fut::Output> {
    fn is_empty(&self) -> bool;
    fn push(&mut self, future: Fut);
}

impl<Fut: Future> FuturesSet<Fut> for FuturesOrdered<Fut> {
    fn is_empty(&self) -> bool {
        Self::is_empty(self)
    }

    fn push(&mut self, future: Fut) {
        Self::push(self, future)
    }
}

impl<Fut: Future> FuturesSet<Fut> for FuturesUnordered<Fut> {
    fn is_empty(&self) -> bool {
        Self::is_empty(self)
    }

    fn push(&mut self, future: Fut) {
        Self::push(self, future)
    }
}

#[pin_project::pin_project]
pub(crate) struct FinalizerFuture<T> {
    receiver: BatchStatusReceiver,
    entry: Option<T>,
}

impl<T> Future for FinalizerFuture<T> {
    type Output = (<BatchStatusReceiver as Future>::Output, T);
    fn poll(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        let status = futures::ready!(self.receiver.poll_unpin(ctx));
        // The use of this above in a `Futures{Ordered|Unordered}`
        // will only take this once before dropping the future.
        Poll::Ready((status, self.entry.take().unwrap_or_else(|| unreachable!())))
    }
}
