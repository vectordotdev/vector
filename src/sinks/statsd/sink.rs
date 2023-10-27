use std::{fmt, future::ready};

use async_trait::async_trait;
use futures_util::{
    stream::{self, BoxStream},
    StreamExt,
};
use tower::Service;
use vector_lib::internal_event::Protocol;
use vector_lib::stream::{BatcherSettings, DriverResponse};
use vector_lib::{event::Event, sink::StreamSink};

use crate::sinks::util::SinkBuilderExt;

use super::{
    batch::StatsdBatchSizer, normalizer::StatsdNormalizer, request_builder::StatsdRequestBuilder,
    service::StatsdRequest,
};

pub(crate) struct StatsdSink<S> {
    service: S,
    batch_settings: BatcherSettings,
    request_builder: StatsdRequestBuilder,
    protocol: Protocol,
}

impl<S> StatsdSink<S>
where
    S: Service<StatsdRequest> + Send,
    S::Error: fmt::Debug + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse,
{
    /// Creates a new `StatsdSink`.
    pub const fn new(
        service: S,
        batch_settings: BatcherSettings,
        request_builder: StatsdRequestBuilder,
        protocol: Protocol,
    ) -> Self {
        Self {
            service,
            batch_settings,
            request_builder,
            protocol,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            // Convert `Event` to `Metric` so we don't have to deal with constant conversions.
            .filter_map(|event| ready(event.try_into_metric()))
            // Converts absolute counters into incremental counters, but otherwise leaves everything
            // else alone. The encoder will handle the difference in absolute vs incremental for
            // other metric types in type-specific ways i.e. incremental gauge updates use a
            // different syntax, etc.
            .normalized_with_default::<StatsdNormalizer>()
            .batched(self.batch_settings.as_item_size_config(StatsdBatchSizer))
            // We build our requests "incrementally", which means that for a single batch of
            // metrics, we might generate N requests to represent all of the metrics in the batch.
            //
            // We do this as for different socket modes, there are optimal request sizes to use to
            // ensure the highest rate of delivery, such as staying within the MTU for UDP, etc.
            .incremental_request_builder(self.request_builder)
            // This unrolls the vector of request results that our request builder generates.
            .flat_map(stream::iter)
            // Generating requests _cannot_ fail, so we just unwrap our built requests.
            .unwrap_infallible()
            // Finally, we generate the driver which will take our requests, send them off, and appropriately handle
            // finalization of the events, and logging/metrics, as the requests are responded to.
            .into_driver(self.service)
            .protocol(self.protocol)
            .run()
            .await
    }
}

#[async_trait]
impl<S> StreamSink<Event> for StatsdSink<S>
where
    S: Service<StatsdRequest> + Send,
    S::Error: fmt::Debug + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        // Rust has issues with lifetimes and generics, which `async_trait` exacerbates, so we write
        // a normal async fn in `StatsdSink` itself, and then call out to it from this trait
        // implementation, which makes the compiler happy.
        self.run_inner(input).await
    }
}
