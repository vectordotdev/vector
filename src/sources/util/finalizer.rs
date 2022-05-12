use std::marker::{PhantomData, Unpin};
use std::{future::Future, pin::Pin, task::Context, task::Poll};

use futures::stream::{FuturesOrdered, FuturesUnordered};
use futures::{FutureExt, Stream, StreamExt};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::event::{BatchStatus, BatchStatusReceiver};
use crate::shutdown::ShutdownSignal;

/// The `OrderedFinalizer` framework produces a stream of acknowledged
/// event batch identifiers from a source in a single background task
/// *in the order they are received from the source*, using
/// `FinalizerSet`.
#[cfg(any(
    feature = "sources-file",
    feature = "sources-journald",
    feature = "sources-kafka",
))]
pub(crate) type OrderedFinalizer<T> = FinalizerSet<T, FuturesOrdered<FinalizerFuture<T>>>;

/// The `UnorderedFinalizer` framework produces a stream of
/// acknowledged event batch identifiers from a source in a single
/// background task *in the order that finalization happens on the
/// event batches*, using `FinalizerSet`.
#[cfg(any(
    feature = "sources-aws_sqs",
    feature = "sources-splunk_hec",
    feature = "sources-gcp_pubsub"
))]
pub(crate) type UnorderedFinalizer<T> = FinalizerSet<T, FuturesUnordered<FinalizerFuture<T>>>;

/// The `FinalizerSet` framework here is a mechanism for creating a
/// stream of acknowledged (finalized) event batch identifiers from a
/// source as done in a single background task. It does this by
/// pushing the batch status receiver along with an identifier into
/// either a `FuturesOrdered` or `FuturesUnordered`, waiting on the
/// stream of acknowledgements that comes out, extracting just the
/// identifier and sending that into the returned stream. The type `T`
/// is the source-specific data associated with each entry.
pub(crate) struct FinalizerSet<T, S> {
    sender: Option<UnboundedSender<(BatchStatusReceiver, T)>>,
    _phantom: PhantomData<S>,
}

impl<T, S> FinalizerSet<T, S>
where
    T: Send + 'static,
    S: FuturesSet<FinalizerFuture<T>> + Default + Send + Unpin + 'static,
{
    /// Produce a finalizer set along with the output stream of
    /// received acknowledged batch identifiers.
    pub(crate) fn new(
        shutdown: ShutdownSignal,
    ) -> (Self, impl Stream<Item = T> + Send + Unpin + 'static) {
        let (todo_tx, todo_rx) = mpsc::unbounded_channel();
        let (done_tx, done_rx) = mpsc::unbounded_channel();
        tokio::spawn(run_finalizer(shutdown, todo_rx, done_tx, S::default()));
        (
            Self {
                sender: Some(todo_tx),
                _phantom: Default::default(),
            },
            UnboundedReceiverStream::new(done_rx),
        )
    }

    /// This returns an optional finalizer set along with a generic
    /// stream of acknowledged identifiers. In the case the finalizer
    /// is not to be used, a special empty stream is returned that is
    /// always pending and so never wakes.
    #[cfg(any(feature = "sources-gcp_pubsub", feature = "sources-kafka"))]
    pub(crate) fn maybe_new(
        maybe: bool,
        shutdown: ShutdownSignal,
    ) -> (
        Option<Self>,
        Pin<Box<dyn Stream<Item = T> + Send + 'static>>,
    ) {
        if maybe {
            let (finalizer, stream) = Self::new(shutdown);
            (Some(finalizer), stream.boxed())
        } else {
            (None, EmptyStream(Default::default()).boxed())
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

async fn run_finalizer<T>(
    shutdown: ShutdownSignal,
    mut new_entries: UnboundedReceiver<(BatchStatusReceiver, T)>,
    done_entries: UnboundedSender<T>,
    mut status_receivers: impl FuturesSet<FinalizerFuture<T>> + Unpin,
) {
    loop {
        tokio::select! {
            _ = shutdown.clone() => break,
            // We could eliminate this `new_entries` channel by just
            // pushing new entries directly into the
            // `status_receivers` except for two problems: 1. The
            // `status_receivers` needs to be `mut` for both pushing
            // entries in and polling the stream, and the locking
            // required to solve that could cause long lock pauses,
            // and 2. The `OrderedFutures`/`UnorderedFutures` types
            // produce an unending stream of `None` when they are
            // empty, and there is no async way to wait for them to
            // not be empty.
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
                Some((status, entry)) => if status == BatchStatus::Delivered && done_entries.send(entry).is_err() {
                    // The receiver went away before shutdown, so
                    // just close up shop as there is nothing more
                    // we can do here.
                    return;
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
        if status == BatchStatus::Delivered && done_entries.send(entry).is_err() {
            break;
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
    fn poll(mut self: Pin<&mut Self>, ctx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let status = futures::ready!(self.receiver.poll_unpin(ctx));
        // The use of this above in a `Futures{Ordered|Unordered|`
        // will only take this once before dropping the future.
        Poll::Ready((status, self.entry.take().unwrap_or_else(|| unreachable!())))
    }
}

#[derive(Clone, Copy)]
struct EmptyStream<T>(PhantomData<T>);

impl<T> Stream for EmptyStream<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Pending
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(0))
    }
}
