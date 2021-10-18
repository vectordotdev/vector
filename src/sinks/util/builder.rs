use std::{fmt, future::Future, hash::Hash, num::NonZeroUsize, pin::Pin, sync::Arc};

use futures_util::Stream;
use tower::Service;
use vector_core::{
    buffers::{Ackable, Acker},
    event::{EventStatus, Finalizable},
    partition::Partitioner,
    stream::{Batcher, BatcherSettings, ConcurrentMap, Driver, ExpirationQueue},
    ByteSizeOf,
};

use super::RequestBuilder;

impl<T: ?Sized> SinkBuilderExt for T where T: Stream {}

pub trait SinkBuilderExt: Stream {
    /// Batches the stream based on the given partitioner and batch settings.
    ///
    /// The stream will yield batches of events, with their partition key, when either a batch fills
    /// up or times out. [`Partitioner`] operates on a per-event basis, and has access to the event
    /// itself, and so can access any and all fields of an event.
    fn batched<P>(
        self,
        partitioner: P,
        settings: BatcherSettings,
    ) -> Batcher<Self, P, ExpirationQueue<P::Key>>
    where
        Self: Stream<Item = P::Item> + Sized + Unpin,
        P: Partitioner + Unpin,
        P::Key: Eq + Hash + Clone,
        P::Item: ByteSizeOf,
    {
        Batcher::new(self, partitioner, settings)
    }

    /// Maps the items in the stream concurrently, up to the configured limit.
    ///
    /// For every item, the given mapper is invoked, and the future that is returned is spawned
    /// and awaited concurrently.  A limit can be passed: `None` is self-describing, as it imposes
    /// no concurrency limit, and `Some(n)` limits this stage to `n` concurrent operations at any
    /// given time.
    ///
    /// If the spawned future panics, the panic will be carried through and resumed on the task
    /// calling the stream.
    fn concurrent_map<F, T>(self, limit: Option<NonZeroUsize>, f: F) -> ConcurrentMap<Self, T>
    where
        Self: Sized,
        // TODO: remove the Pin<Box<_>>
        F: Fn(Self::Item) -> Pin<Box<dyn Future<Output = T> + Send + 'static>> + Send + 'static,
        T: Send + 'static,
    {
        ConcurrentMap::new(self, limit, f)
    }

    /// Constructs a [`Stream`] which transforms the input into a request suitable for sending to
    /// downstream services.
    ///
    /// Each input is transformed concurrently, up to the given limit.  A limit of `None` is
    /// self-describing, as it imposes no concurrency limit, and `Some(n)` limits this stage to `n`
    /// concurrent operations at any given time.
    ///
    /// Encoding and compression are handled internally, deferring to the builder at the necessary
    /// checkpoints for adjusting the event before encoding/compression, as well as generating the
    /// correct request object with the result of encoding/compressing the events.
    fn request_builder<B>(
        self,
        limit: Option<NonZeroUsize>,
        builder: B,
    ) -> ConcurrentMap<Self, Result<B::Request, B::Error>>
    where
        Self: Sized,
        Self::Item: Send + 'static,
        B: RequestBuilder<<Self as Stream>::Item> + Send + Sync + 'static,
        B::Error: Send,
        B::Request: Send,
    {
        let builder = Arc::new(builder);

        self.concurrent_map(limit, move |input| {
            let builder = Arc::clone(&builder);

            Box::pin(async move {
                // Split the input into metadata and events.
                let (metadata, events) = builder.split_input(input);

                // Encode the events.
                let payload = builder.encode_events(events)?;

                // Now build the actual request.
                Ok(builder.build_request(metadata, payload))
            })
        })
    }

    /// Creates a [`Driver`] that uses the configured event stream as the input to the given
    /// service.
    ///
    /// This is typically a terminal step in building a sink, bridging the gap from the processing
    /// that must be performed by Vector (in the stream) to the underlying sink itself (the
    /// service).
    ///
    /// As it is intended to be a terminal step, we require an [`Acker`] in order to be able to
    /// provide acking based on the responses from the underlying service.
    fn into_driver<Svc>(self, service: Svc, acker: Acker) -> Driver<Self, Svc>
    where
        Self: Sized,
        Self::Item: Ackable + Finalizable,
        Svc: Service<Self::Item>,
        Svc::Error: fmt::Debug + 'static,
        Svc::Future: Send + 'static,
        Svc::Response: AsRef<EventStatus>,
    {
        Driver::new(self, service, acker)
    }
}
