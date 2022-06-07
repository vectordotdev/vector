use std::marker::{PhantomData, Unpin};
use std::{fmt::Debug, future::Future, pin::Pin, task::Context, task::Poll};

use futures::stream::{FuturesOrdered, FuturesUnordered};
use futures::{FutureExt, Stream, StreamExt};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

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
    T: Send + Debug + 'static,
    S: FuturesSet<FinalizerFuture<T>> + Default + Send + Unpin + 'static,
{
    /// Produce a finalizer set along with the output stream of
    /// received acknowledged batch identifiers.
    pub(crate) fn new(shutdown: ShutdownSignal) -> (Self, impl Stream<Item = (BatchStatus, T)>) {
        let (todo_tx, todo_rx) = mpsc::unbounded_channel();
        (
            Self {
                sender: Some(todo_tx),
                _phantom: Default::default(),
            },
            FinalizerStream {
                shutdown,
                new_entries: todo_rx,
                status_receivers: S::default(),
                is_shutdown: false,
            },
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
        Pin<Box<dyn Stream<Item = (BatchStatus, T)> + Send + 'static>>,
    ) {
        if maybe {
            let (finalizer, stream) = Self::new(shutdown);
            (Some(finalizer), stream.boxed())
        } else {
            (None, EmptyStream::default().boxed())
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

#[pin_project::pin_project]
#[derive(Debug)]
struct FinalizerStream<T, S> {
    shutdown: ShutdownSignal,
    new_entries: UnboundedReceiver<(BatchStatusReceiver, T)>,
    status_receivers: S,
    is_shutdown: bool,
}

impl<T, S> Stream for FinalizerStream<T, S>
where
    S: FuturesSet<FinalizerFuture<T>> + Unpin,
    T: Debug,
{
    type Item = (BatchStatus, T);

    fn poll_next(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        if !*this.is_shutdown {
            if this.shutdown.poll_unpin(ctx).is_ready() {
                *this.is_shutdown = true
            }
            // Only poll for new entries until shutdown is flagged.
            // Loop over all the ready new entries at once.
            loop {
                match this.new_entries.poll_recv(ctx) {
                    Poll::Pending => break,
                    Poll::Ready(Some((receiver, entry))) => {
                        let entry = Some(entry);
                        this.status_receivers
                            .push(FinalizerFuture { receiver, entry });
                    }
                    // The sender went away before shutdown, count it as a shutdown too.
                    Poll::Ready(None) => {
                        *this.is_shutdown = true;
                        break;
                    }
                }
            }
        }

        match this.status_receivers.poll_next_unpin(ctx) {
            Poll::Pending => Poll::Pending,
            // The futures set report `None` ready when there are no
            // entries present, but we want it to report pending
            // instead.
            Poll::Ready(None) => {
                if *this.is_shutdown {
                    Poll::Ready(None)
                } else {
                    Poll::Pending
                }
            }
            Poll::Ready(Some((status, entry))) => Poll::Ready(Some((status, entry))),
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

#[derive(Clone, Copy, Derivative)]
#[derivative(Default(bound = ""))]
pub struct EmptyStream<T>(PhantomData<T>);

impl<T> Stream for EmptyStream<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Pending
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(0))
    }
}
