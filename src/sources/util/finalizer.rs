use crate::event::BatchStatusReceiver;
use futures::{stream::FuturesUnordered, FutureExt, StreamExt};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::task::Poll;
use stream_cancel::{Trigger, Tripwire};
use tokio::sync::mpsc;

pub(crate) struct OrderedFinalizer<T> {
    sender: Option<mpsc::UnboundedSender<(BatchStatusReceiver, T)>>,
    shutdown_done: Tripwire,
}

impl<T: Send + 'static> OrderedFinalizer<T> {
    pub(crate) fn new(apply_done: impl Fn(T) + Send + 'static) -> Self {
        let (shutdown_trigger, shutdown_done) = Tripwire::new();
        let (sender, receiver) = mpsc::unbounded_channel();
        tokio::spawn(run_finalizer(shutdown_trigger, receiver, apply_done));
        Self {
            sender: Some(sender),
            shutdown_done,
        }
    }

    pub(crate) fn shutdown_done(&self) -> Tripwire {
        self.shutdown_done.clone()
    }

    pub(crate) fn start_shutdown(&mut self) {
        self.sender.take();
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
    shutdown: Trigger,
    mut receiver: mpsc::UnboundedReceiver<(BatchStatusReceiver, T)>,
    apply_done: impl Fn(T),
) {
    let mut receivers = FuturesUnordered::default();
    let mut store = FinalizerStore::new();

    loop {
        if receivers.is_empty() {
            match receiver.recv().await {
                Some((receiver, entry)) => {
                    let index = store.add_entry(entry);
                    receivers.push(FinalizerFuture { receiver, index });
                }
                None => break,
            }
        } else {
            tokio::select! {
                received = receiver.recv() => match received {
                    Some((receiver, entry)) => {
                        let index = store.add_entry(entry);
                        receivers.push(FinalizerFuture { receiver, index });
                    }
                    None => break,
                },
                result = receivers.next() => match result {
                    Some((_status, index)) => {
                        store.mark_done(index);
                        store.extract_done(&apply_done);
                    }
                    None => break,
                },
            }
        }
    }
    while let Some((_status, index)) = receivers.next().await {
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
