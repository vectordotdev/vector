use std::marker::{PhantomData, Unpin};
use std::{future::Future, pin::Pin, task::Poll};

use futures::stream::{FuturesOrdered, FuturesUnordered};
use futures::{future::Shared, FutureExt, Stream, StreamExt};
use tokio::sync::mpsc;

use crate::event::{BatchStatus, BatchStatusReceiver};
use crate::shutdown::ShutdownSignal;

/// The `OrderedFinalizer` framework marks events from a source as
/// done in a single background task *in the order they are received
/// from the source*, using `FinalizerSet`.
#[cfg(any(feature = "sources-file", feature = "sources-kafka",))]
pub(crate) type OrderedFinalizer<T> = FinalizerSet<T, FuturesOrdered<FinalizerFuture<T>>>;

/// The `UnorderedFinalizer` framework marks events from a source as
/// done in a single background task *in the order the finalization
/// happens on the event batches*, using `FinalizerSet`.
#[cfg(any(feature = "sources-aws_sqs", feature = "sources-splunk_hec"))]
pub(crate) type UnorderedFinalizer<T> = FinalizerSet<T, FuturesUnordered<FinalizerFuture<T>>>;

/// The `FinalizerSet` framework here is a mechanism for marking
/// events from a source as done in a single background task. The type
/// `T` is the source-specific data associated with each entry to be
/// used to complete the finalization.
pub(crate) struct FinalizerSet<T, S> {
    sender: Option<mpsc::UnboundedSender<(BatchStatusReceiver, T)>>,
    _phantom: PhantomData<S>,
}

impl<T, S> FinalizerSet<T, S>
where
    T: Send + 'static,
    S: FuturesSet<FinalizerFuture<T>> + Default + Send + Unpin + 'static,
{
    pub(crate) fn new<F, Fut>(shutdown: Shared<ShutdownSignal>, apply_done: F) -> Self
    where
        F: Fn(T) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let (sender, receiver) = mpsc::unbounded_channel();
        tokio::spawn(run_finalizer(shutdown, receiver, apply_done, S::default()));
        Self {
            sender: Some(sender),
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
}

async fn run_finalizer<T, F: Future<Output = ()>>(
    shutdown: Shared<ShutdownSignal>,
    mut new_entries: mpsc::UnboundedReceiver<(BatchStatusReceiver, T)>,
    apply_done: impl Fn(T) -> F,
    mut status_receivers: impl FuturesSet<FinalizerFuture<T>> + Unpin,
) {
    loop {
        tokio::select! {
            _ = shutdown.clone() => break,
            new_entry = new_entries.recv() => match new_entry {
                Some((receiver, entry)) => {
                    status_receivers.push(FinalizerFuture {
                        receiver,
                        entry: Some(entry),
                    });
                }
                None => break,
            },
            finished = status_receivers.next(), if !status_receivers.is_empty() => match finished {
                Some((status, entry)) => if status == BatchStatus::Delivered {
                    apply_done(entry).await;
                }
                // The is_empty guard above prevents this from being reachable.
                None => unreachable!(),
            },
        }
    }
    // We've either seen a shutdown signal or the new entry sender was
    // closed. Wait for the last statuses to come in before indicating
    // we are done.
    while let Some((status, entry)) = status_receivers.next().await {
        if status == BatchStatus::Delivered {
            apply_done(entry).await;
        }
    }
    drop(shutdown);
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
    fn poll(mut self: Pin<&mut Self>, ctx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let status = futures::ready!(self.receiver.poll_unpin(ctx));
        // The use of this above in a `Futures{Ordered|Unordered|`
        // will only take this once before dropping the future.
        Poll::Ready((status, self.entry.take().unwrap_or_else(|| unreachable!())))
    }
}
