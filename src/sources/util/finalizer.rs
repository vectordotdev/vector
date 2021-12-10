use crate::event::BatchStatusReceiver;
use crate::shutdown::ShutdownSignal;
use futures::{future::Shared, stream::FuturesOrdered, FutureExt, StreamExt};
use std::{future::Future, pin::Pin, sync::Arc, task::Poll};
use tokio::sync::{mpsc, Notify};

/// The `OrderedFinalizer` framework here is a mechanism for marking
/// events from a source as done in a single background task *in the
/// order they are received from the source*. The type `T` is the
/// source-specific data associated with each entry to be used to
/// complete the finalization.
pub struct OrderedFinalizer<T> {
    sender: Option<mpsc::UnboundedSender<(BatchStatusReceiver, T)>>,
    flush: Option<Arc<Notify>>,
}

impl<T: Send + 'static> OrderedFinalizer<T> {
    pub(crate) fn new(
        shutdown: Shared<ShutdownSignal>,
        apply_done: impl Fn(T) + Send + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        let flush = Arc::new(Notify::new());
        tokio::spawn(run_finalizer(
            shutdown,
            receiver,
            apply_done,
            Arc::clone(&flush),
        ));
        Self {
            sender: Some(sender),
            flush: Some(flush),
        }
    }

    pub(crate) fn add(&self, entry: T, receiver: BatchStatusReceiver) {
        if let Some(sender) = &self.sender {
            if let Err(error) = sender.send((receiver, entry)) {
                error!(message = "OrderedFinalizer task ended prematurely.", %error);
            }
        }
    }

    pub(crate) fn flush(&self) {
        if let Some(flush) = &self.flush {
            flush.notify_one();
        }
    }
}

async fn run_finalizer<T>(
    shutdown: Shared<ShutdownSignal>,
    mut new_entries: mpsc::UnboundedReceiver<(BatchStatusReceiver, T)>,
    apply_done: impl Fn(T),
    flush: Arc<Notify>,
) {
    let mut status_receivers = FuturesOrdered::default();

    loop {
        tokio::select! {
            _ = shutdown.clone() => break,
            _ = flush.notified() => {
                // Drop all the existing status receivers and start over.
                status_receivers = FuturesOrdered::default();
            },
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
                Some((_status, entry)) => apply_done(entry),
                // The is_empty guard above prevents this from being reachable.
                None => unreachable!(),
            },
        }
    }
    // We've either seen a shutdown signal or the new entry sender was
    // closed. Wait for the last statuses to come in before indicating
    // we are done.
    while let Some((_status, entry)) = status_receivers.next().await {
        apply_done(entry);
    }
    drop(shutdown);
}

#[pin_project::pin_project]
struct FinalizerFuture<T> {
    receiver: BatchStatusReceiver,
    entry: Option<T>,
}

impl<T> Future for FinalizerFuture<T> {
    type Output = (<BatchStatusReceiver as Future>::Output, T);
    fn poll(mut self: Pin<&mut Self>, ctx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let status = futures::ready!(self.receiver.poll_unpin(ctx));
        // The use of this above in a `FuturesOrdered` will only take
        // this once before dropping the future.
        Poll::Ready((status, self.entry.take().unwrap_or_else(|| unreachable!())))
    }
}
