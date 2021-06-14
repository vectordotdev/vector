use crate::event::BatchStatusReceiver;
use crate::shutdown::ShutdownSignal;
use futures::{future::Shared, stream::FuturesUnordered, FutureExt, StreamExt};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::task::Poll;
use tokio::sync::mpsc;

/// The `OrderedFinalizer` framework here is a mechanism for marking
/// events from a source as done in a single background task *in the
/// order they are received from the source*. The type `T` is the
/// source-specific data associated with each entry to be used to
/// complete the finalization.
pub(crate) struct OrderedFinalizer<T> {
    sender: Option<mpsc::UnboundedSender<(BatchStatusReceiver, T)>>,
}

// TODO: Rewrite `apply_done` below into a trait once there are more
// than one user of this framework. This works for now.

impl<T: Send + 'static> OrderedFinalizer<T> {
    pub(crate) fn new(
        shutdown: Shared<ShutdownSignal>,
        apply_done: impl Fn(T) + Send + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        tokio::spawn(run_finalizer(shutdown, receiver, apply_done));
        Self {
            sender: Some(sender),
        }
    }

    pub(crate) fn add(&self, entry: T, receiver: BatchStatusReceiver) {
        if let Some(sender) = &self.sender {
            if let Err(error) = sender.send((receiver, entry)) {
                error!(message = "OrderedFinalizer task ended prematurely.", %error);
            }
        }
    }
}

async fn run_finalizer<T>(
    shutdown: Shared<ShutdownSignal>,
    mut new_entries: mpsc::UnboundedReceiver<(BatchStatusReceiver, T)>,
    apply_done: impl Fn(T),
) {
    let mut status_receivers = FuturesUnordered::default();
    let mut store = FinalizerStore::new();

    loop {
        tokio::select! {
            _ = shutdown.clone() => break,
            new_entry = new_entries.recv() => match new_entry {
                Some((receiver, entry)) => {
                    let index = store.add_entry(entry);
                    status_receivers.push(FinalizerFuture { receiver, index });
                }
                None => break,
            },
            finished = status_receivers.next(), if !status_receivers.is_empty() => match finished {
                Some((_status, index)) => {
                    store.mark_done(index);
                    store.extract_done(&apply_done);
                }
                // The is_empty guard above prevents this from being reachable.
                None => unreachable!(),
            },
        }
    }
    // We've either seen a shutdown signal or the new entry sender was
    // closed. Wait for the last statuses to come in before indicating
    // we are done.
    while let Some((_status, index)) = status_receivers.next().await {
        store.mark_done(index);
        store.extract_done(&apply_done);
    }
    drop(shutdown);
}

struct FinalizerFuture {
    receiver: BatchStatusReceiver,
    index: u64,
}

impl Future for FinalizerFuture {
    type Output = (<BatchStatusReceiver as Future>::Output, u64);
    fn poll(mut self: Pin<&mut Self>, ctx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        self.receiver
            .poll_unpin(ctx)
            .map(|status| (status, self.index))
    }
}

struct FinalizerStore<T> {
    first: u64,
    entries: VecDeque<(bool, T)>,
}

impl<T> FinalizerStore<T> {
    fn new() -> Self {
        Self {
            first: 0,
            entries: VecDeque::new(),
        }
    }

    /// Add the needed data from the given message into the store and
    /// return the index.
    #[must_use]
    fn add_entry(&mut self, entry: T) -> u64 {
        let index = self.first + self.entries.len() as u64;
        self.entries.push_back((false, entry));
        index
    }

    fn mark_done(&mut self, index: u64) {
        // Under normal usage, both of these conditions should be true,
        // but they are guarded to avoid panics.
        if index >= self.first {
            let offset = (index - self.first) as usize;
            if let Some(entry) = self.entries.get_mut(offset) {
                entry.0 = true;
            }
        }
    }

    fn extract_done(&mut self, apply: impl Fn(T)) {
        while let Some(entry) = self.entries.front() {
            if !entry.0 {
                break;
            }
            let entry = self.entries.pop_front().unwrap().1;
            self.first += 1;
            apply(entry);
        }
    }
}
