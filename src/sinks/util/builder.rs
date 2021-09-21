use std::{fmt, future::Future, num::NonZeroUsize, time::Duration};

use futures_util::{Stream, StreamExt, stream::FilterMap};
use tower::Service;
use vector_core::{buffers::{Ackable, Acker}, event::{EventStatus, Finalizable}, partition::Partitioner, stream::{batcher::{Batcher, ExpirationQueue}, driver::Driver}};

use super::ConcurrentMap;

pub struct SinkBuilder<St> {
    inner: St,
}

impl<St> SinkBuilder<St>
where
    St: Stream,
{
    pub fn new(stream: St) -> SinkBuilder<St> {
        SinkBuilder {
            inner: stream,
        }
    }

    /// Consumes the `SinkBuilder`, returning the wrapped stream.
    pub fn into_inner(self) -> St {
        self.inner
    }

    /// Batches the stream based on the given partitioner and batch settings.
    ///
    /// The stream will yield batches of events, with their partition key, when either a batch fills
    /// up or times out. [`Partitioner`] operates on a per-event basis, and has access to the event
    /// itself, and so can access any and all fields of an event.
    pub fn batched<P>(
        self,
        partitioner: P,
        batch_timeout: Duration,
        batch_item_limit: NonZeroUsize,
        batch_allocation_limit: Option<NonZeroUsize>
    ) -> SinkBuilder<Batcher<St, P, ExpirationQueue<P::Key>>>
    where
        P: Partitioner + Unpin,
        St: Unpin,
    {
        SinkBuilder {
            inner: Batcher::new(self.inner, partitioner, batch_timeout, batch_item_limit, batch_allocation_limit),
        }
    }

    /// Filters the values produced by this stream while simultaneously mapping them to a different
    /// type according to the provided asynchronous closure.
    ///
    /// As values of this stream are made available, the provided function will be run on them. If
    /// the future returned by the predicate `f` resolves to `Some(item)` then the stream will yield the
    /// value item, but if it resolves to `None` then the next value will be produced.
    pub fn filter_map<F, Fut, T>(self, f: F) -> SinkBuilder<FilterMap<St, Fut, F>>
    where
        F: FnMut(St::Item) -> Fut,
        Fut: Future<Output = Option<T>>,
    {
        SinkBuilder {
            inner: self.inner.filter_map(f),
        }
    }

    /// Maps the items in the stream concurrently, up to the configured limit.
    ///
    /// For every item, the given function is invoked, and the future that is returned is spawned
    /// and awaited concurrently.  A limit can be passed: `None` is self-describing, as it imposes
    /// no concurrency limit, and `Some(n)` limits this stage to `n` concurrent operations at any
    /// given time.
    ///
    /// If the spawned future panics, the panic will be carried through and resumed on the task
    /// calling the stream.
    pub fn concurrent_map<F, Fut, T>(self, limit: Option<NonZeroUsize>, f: F) -> SinkBuilder<ConcurrentMap<St, F, Fut, T>>
    where
        F: FnMut(St::Item) -> Fut,
        Fut: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        SinkBuilder {
            inner: ConcurrentMap::new(self.inner, limit, f)
        }
    }

    /// Creates a [`Driver`] that uses the configured event stream as the input to the given
    /// service.
    ///
    /// This is typically a terminal step in building a sink, bridging the gap from the processing
    /// that must be performed by Vector (in the stream) to the underlying sink itself (the
    /// service).  As such, it entirely consumes the `SinkBuilder`, allowing no further composition.
    ///
    /// As it is intended to be a terminal step, we require an [`Acker`] in order to be able to
    /// provide acking based on the responses from the underlying service.
    pub fn driver<Svc>(self, service: Svc, acker: Acker) -> Driver<St, Svc>
    where
        St::Item: Ackable + Finalizable,
        Svc: Service<St::Item>,
        Svc::Error: fmt::Debug + 'static,
        Svc::Future: Send + 'static,
        Svc::Response: AsRef<EventStatus>,
    {
        Driver::new(self.inner, service, acker)
    }
}
