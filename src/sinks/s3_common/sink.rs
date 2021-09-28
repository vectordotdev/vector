use crate::{
    config::SinkContext,
    event::Event,
    sinks::util::{encoding::Encoder, Compression, RequestBuilder, SinkBuilderExt},
};
use async_trait::async_trait;
use futures::stream::BoxStream;
use futures_util::StreamExt;
use std::{fmt, num::NonZeroUsize, time::Duration};
use tower::Service;
use vector_core::{buffers::Ackable, event::Finalizable, sink::StreamSink};
use vector_core::{buffers::Acker, event::EventStatus};

use crate::sinks::s3_common::partitioner::KeyPartitioner;
use std::fmt::Debug;

pub struct S3Sink<Svc, RB, E> {
    acker: Acker,
    service: Svc,
    request_builder: RB,
    partitioner: KeyPartitioner,
    encoding: E,
    compression: Compression,
    batch_size_bytes: Option<NonZeroUsize>,
    batch_size_events: NonZeroUsize,
    batch_timeout: Duration,
}

impl<Svc, RB, E> S3Sink<Svc, RB, E> {
    pub fn new(
        cx: SinkContext,
        service: Svc,
        request_builder: RB,
        partitioner: KeyPartitioner,
        encoding: E,
        compression: Compression,
        batch_size_bytes: Option<NonZeroUsize>,
        batch_size_events: NonZeroUsize,
        batch_timeout: Duration,
    ) -> Self {
        Self {
            partitioner,
            encoding,
            compression,
            acker: cx.acker(),
            service,
            request_builder,
            batch_size_bytes,
            batch_size_events,
            batch_timeout,
        }
    }
}

impl<Svc, RB, E> S3Sink<Svc, RB, E>
where
    Svc: Service<RB::Request> + Send + 'static,
    Svc::Future: Send + 'static,
    Svc::Response: AsRef<EventStatus> + Send + 'static,
    Svc::Error: fmt::Debug + Into<crate::Error> + Send,
    RB: RequestBuilder<(String, Vec<Event>)> + Send + Sync + 'static,
    RB::Events: IntoIterator<Item = Event>,
    RB::Payload: From<Vec<u8>>,
    RB::Request: Ackable + Finalizable + Send,
    RB::SplitError: Send + Debug,
    E: Encoder + Send + Sync + 'static,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        // All sinks do the same fundamental job: take in events, and ship them
        // out. Empirical testing shows that our number one priority for high
        // throughput needs to be servicing I/O as soon as we possibly can.  In
        // order to do that, we'll spin up a separate task that deals
        // exclusively with I/O, while we deal with everything else here:
        // batching, ordering, and so on.

        let request_builder_rate_limit = NonZeroUsize::new(50);

        let sink = input
            .batched(
                self.partitioner,
                self.batch_timeout,
                self.batch_size_events,
                self.batch_size_bytes,
            )
            .filter_map(|(key, batch)| async move { key.map(move |k| (k, batch)) })
            .request_builder(
                request_builder_rate_limit,
                self.request_builder,
                self.encoding,
                self.compression,
            )
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("failed to build S3 request: {:?}", e);
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service, self.acker);

        sink.run().await
    }
}

#[async_trait]
impl<Svc, RB, E> StreamSink for S3Sink<Svc, RB, E>
where
    Svc: Service<RB::Request> + Send + 'static,
    Svc::Future: Send + 'static,
    Svc::Response: AsRef<EventStatus> + Send + 'static,
    Svc::Error: fmt::Debug + Into<crate::Error> + Send,
    RB: RequestBuilder<(String, Vec<Event>)> + Send + Sync + 'static,
    RB::Events: IntoIterator<Item = Event>,
    RB::Payload: From<Vec<u8>>,
    RB::Request: Ackable + Finalizable + Send,
    RB::SplitError: Send + Debug,
    E: Encoder + Send + Sync + 'static,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
