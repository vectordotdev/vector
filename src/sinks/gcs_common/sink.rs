use std::{fmt, num::NonZeroUsize};

use async_trait::async_trait;
use futures::stream::BoxStream;
use futures_util::StreamExt;
use tower::Service;
use vector_core::{
    buffers::{Ackable, Acker},
    event::Finalizable,
    sink::StreamSink,
    stream::{BatcherSettings, DriverResponse},
};

use crate::{
    config::SinkContext,
    event::Event,
    sinks::util::{partitioner::KeyPartitioner, RequestBuilder, SinkBuilderExt},
};

pub struct GcsSink<Svc, RB> {
    acker: Acker,
    service: Svc,
    request_builder: RB,
    partitioner: KeyPartitioner,
    batcher_settings: BatcherSettings,
}

impl<Svc, RB> GcsSink<Svc, RB> {
    pub fn new(
        cx: SinkContext,
        service: Svc,
        request_builder: RB,
        partitioner: KeyPartitioner,
        batcher_settings: BatcherSettings,
    ) -> Self {
        Self {
            acker: cx.acker(),
            service,
            request_builder,
            partitioner,
            batcher_settings,
        }
    }
}

impl<Svc, RB> GcsSink<Svc, RB>
where
    Svc: Service<RB::Request> + Send + 'static,
    Svc::Future: Send + 'static,
    Svc::Response: DriverResponse + Send + 'static,
    Svc::Error: fmt::Debug + Into<crate::Error> + Send,
    RB: RequestBuilder<(String, Vec<Event>)> + Send + Sync + 'static,
    RB::Error: fmt::Debug + Send,
    RB::Request: Ackable + Finalizable + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let partitioner = self.partitioner;
        let settings = self.batcher_settings;

        let builder_limit = NonZeroUsize::new(64);
        let request_builder = self.request_builder;

        let sink = input
            .batched_partitioned(partitioner, settings)
            .filter_map(|(key, batch)| async move { key.map(move |k| (k, batch)) })
            .request_builder(builder_limit, request_builder)
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("Failed to build GCS request: {:?}.", e);
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
impl<Svc, RB> StreamSink<Event> for GcsSink<Svc, RB>
where
    Svc: Service<RB::Request> + Send + 'static,
    Svc::Future: Send + 'static,
    Svc::Response: DriverResponse + Send + 'static,
    Svc::Error: fmt::Debug + Into<crate::Error> + Send,
    RB: RequestBuilder<(String, Vec<Event>)> + Send + Sync + 'static,
    RB::Error: fmt::Debug + Send,
    RB::Request: Ackable + Finalizable + Send,
{
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
