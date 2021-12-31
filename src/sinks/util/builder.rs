use std::{fmt, future::Future, hash::Hash, num::NonZeroUsize, pin::Pin, sync::Arc};

use futures_util::{stream::Map, Stream, StreamExt};
use tower::Service;
use vector_core::{
    buffers::{Ackable, Acker},
    event::{Finalizable, Metric},
    partition::Partitioner,
    stream::{
        batcher::{config::BatchConfig, Batcher},
        BatcherSettings, ConcurrentMap, Driver, DriverResponse, ExpirationQueue,
        PartitionedBatcher,
    },
    ByteSizeOf,
};

use super::{
    buffer::metrics::MetricNormalize, IncrementalRequestBuilder, Normalizer, RequestBuilder,
};

impl<T: ?Sized> SinkBuilderExt for T where T: Stream {}

pub trait SinkBuilderExt: Stream {
    /// Batches the stream based on the given partitioner and batch settings.
    ///
    /// The stream will yield batches of events, with their partition key, when either a batch fills
    /// up or times out. [`Partitioner`] operates on a per-event basis, and has access to the event
    /// itself, and so can access any and all fields of an event.
    fn batched_partitioned<P>(
        self,
        partitioner: P,
        settings: BatcherSettings,
    ) -> PartitionedBatcher<Self, P, ExpirationQueue<P::Key>>
    where
        Self: Stream<Item = P::Item> + Sized,
        P: Partitioner + Unpin,
        P::Key: Eq + Hash + Clone,
        P::Item: ByteSizeOf,
    {
        PartitionedBatcher::new(self, partitioner, settings)
    }

    /// Batches the stream based on the given batch settings and item size calculator.
    ///
    /// The stream will yield batches of events, when either a batch fills
    /// up or times out. The `item_size_calculator` determines the "size" of each input
    /// in a batch. The units of "size" are intentionally not defined, so you can choose
    /// whatever is needed.
    fn batched<C>(self, config: C) -> Batcher<Self, C>
    where
        C: BatchConfig<Self::Item>,
        Self: Sized,
    {
        Batcher::new(self, config)
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

    /// Constructs a [`Stream`] which transforms the input into a number of requests suitable for
    /// sending to downstream services.
    ///
    /// Unlike `request_builder`, which depends on the `RequestBuilder` trait,
    /// `incremental_request_builder` depends on the `IncrementalRequestBuilder` trait, which is
    /// designed specifically for sinks that have more stringent requirements around the generated
    /// requests.
    ///
    /// As an example, the normal `request_builder` doesn't allow for a batch of input events to be
    /// split up: all events must be split at the beginning, encoded separately (and all together),
    /// and the nreassembled into the request.  If the encoding of these events caused a payload to
    /// be generated that was, say, too large, you would have to back out the operation entirely by
    /// failing the batch.
    ///
    /// With `incremental_request_builder`, the builder is given all of the events in a single shot,
    /// and can generate multiple payloads.  This is the maximally flexible approach to encoding,
    /// but means that the trait doesn't provide any default methods like `RequestBuilder` does.
    ///
    /// Each input is transformed serially.
    ///
    /// Encoding and compression are handled internally, deferring to the builder at the necessary
    /// checkpoints for adjusting the event before encoding/compression, as well as generating the
    /// correct request object with the result of encoding/compressing the events.
    fn incremental_request_builder<B>(
        self,
        mut builder: B,
    ) -> Map<Self, Box<dyn FnMut(Self::Item) -> Vec<Result<B::Request, B::Error>> + Send + Sync>>
    where
        Self: Sized,
        Self::Item: Send + 'static,
        B: IncrementalRequestBuilder<<Self as Stream>::Item> + Send + Sync + 'static,
        B::Error: Send,
        B::Request: Send,
    {
        self.map(Box::new(move |input| {
            builder
                .encode_events_incremental(input)
                .into_iter()
                .map(|result| {
                    result.map(|(metadata, payload)| builder.build_request(metadata, payload))
                })
                .collect()
        }))
    }

    /// Normalizes a stream of [`Metric`] events with the provided normalizer.
    ///
    /// An implementation of [`MetricNormalize`] is used to either drop metrics which cannot be
    /// supported by the sink, or to modify them.  Such modifications typically include converting
    /// absolute metrics to incremental metrics by tracking the change over time for a particular
    /// series, or emitting absolute metrics based on incremental updates.
    fn normalized<N>(self) -> Normalizer<Self, N>
    where
        Self: Stream<Item = Metric> + Unpin + Sized,
        N: MetricNormalize,
    {
        Normalizer::new(self)
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
        Svc::Response: DriverResponse,
    {
        Driver::new(self, service, acker)
    }
}
