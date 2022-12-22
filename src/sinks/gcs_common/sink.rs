use std::{fmt, num::NonZeroUsize};

use async_trait::async_trait;
use futures::stream::BoxStream;
use futures_util::StreamExt;
use tower::Service;
use vector_common::request_metadata::MetaDescriptive;
use vector_core::{
    event::Finalizable,
    sink::StreamSink,
    stream::{BatcherSettings, DriverResponse},
};

use crate::{
    event::Event,
    internal_events::SinkRequestBuildError,
    sinks::util::{partitioner::KeyPartitioner, RequestBuilder, SinkBuilderExt},
};

pub struct GcsSink<Svc, RB> {
    service: Svc,
    request_builder: RB,
    partitioner: KeyPartitioner,
    batcher_settings: BatcherSettings,
    protocol: &'static str,
}

impl<Svc, RB> GcsSink<Svc, RB> {
    pub const fn new(
        service: Svc,
        request_builder: RB,
        partitioner: KeyPartitioner,
        batcher_settings: BatcherSettings,
        protocol: &'static str,
    ) -> Self {
        Self {
            service,
            request_builder,
            partitioner,
            batcher_settings,
            protocol,
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
    RB::Error: fmt::Display + Send,
    RB::Request: Finalizable + MetaDescriptive + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let partitioner = self.partitioner;
        let settings = self.batcher_settings;

        let builder_limit = NonZeroUsize::new(64);
        let request_builder = self.request_builder;

        input
            .batched_partitioned(partitioner, settings)
            .filter_map(|(key, batch)| async move {
                // A `TemplateRenderingError` will have been emitted by `KeyPartitioner` if the key here is `None`,
                // thus no further `EventsDropped` event needs emitting at this stage.
                key.map(move |k| (k, batch))
            })
            .request_builder(builder_limit, request_builder)
            .filter_map(|request| async move {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service)
            .protocol(self.protocol)
            .run()
            .await
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
    RB::Error: fmt::Display + Send,
    RB::Request: Finalizable + MetaDescriptive + Send,
{
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
