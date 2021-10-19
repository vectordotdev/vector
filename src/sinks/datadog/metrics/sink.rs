use std::{fmt, num::NonZeroUsize};

use async_trait::async_trait;
use futures_util::{
    future::ready,
    stream::{self, BoxStream},
    StreamExt,
};
use tower::Service;
use vector_core::{
    buffers::Acker,
    event::{Event, EventStatus, Metric, MetricValue},
    partition::Partitioner,
    sink::StreamSink,
    stream::BatcherSettings,
};

use crate::{config::SinkContext, sinks::util::SinkBuilderExt};

use super::{
    config::DatadogMetricsEndpoint, normalizer::DatadogMetricsNormalizer,
    request_builder::DatadogMetricsRequestBuilder, service::DatadogMetricsRequest,
};

struct DatadogMetricsTypePartitioner;

impl Partitioner for DatadogMetricsTypePartitioner {
    type Item = Metric;
    type Key = DatadogMetricsEndpoint;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        match item.data().value() {
            MetricValue::Counter { .. } => DatadogMetricsEndpoint::Series,
            MetricValue::Gauge { .. } => DatadogMetricsEndpoint::Series,
            MetricValue::Set { .. } => DatadogMetricsEndpoint::Series,
            MetricValue::Distribution { .. } => DatadogMetricsEndpoint::Distribution,
            MetricValue::AggregatedHistogram { .. } => DatadogMetricsEndpoint::Sketch,
            MetricValue::AggregatedSummary { .. } => DatadogMetricsEndpoint::Series,
            MetricValue::Sketch { .. } => DatadogMetricsEndpoint::Sketch,
        }
    }
}

pub struct DatadogMetricsSink<S> {
    service: S,
    acker: Acker,
    request_builder: DatadogMetricsRequestBuilder,
    batch_settings: BatcherSettings,
}

impl<S> DatadogMetricsSink<S>
where
    S: Service<DatadogMetricsRequest> + Send,
    S::Error: fmt::Debug + 'static,
    S::Future: Send + 'static,
    S::Response: AsRef<EventStatus>,
{
    pub fn new(
        cx: SinkContext,
        service: S,
        request_builder: DatadogMetricsRequestBuilder,
        batch_settings: BatcherSettings,
    ) -> Self {
        DatadogMetricsSink {
            service,
            acker: cx.acker(),
            request_builder,
            batch_settings,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let builder_limit = NonZeroUsize::new(64);

        let sink = input
            .filter_map(|event| ready(event.try_into_metric()))
            .normalized::<DatadogMetricsNormalizer>()
            .batched(DatadogMetricsTypePartitioner, self.batch_settings)
            .incremental_request_builder(builder_limit, self.request_builder)
            .flat_map(stream::iter)
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("Failed to build Datadog Metrics request: {:?}.", e);
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
impl<S> StreamSink for DatadogMetricsSink<S>
where
    S: Service<DatadogMetricsRequest> + Send,
    S::Error: fmt::Debug + 'static,
    S::Future: Send + 'static,
    S::Response: AsRef<EventStatus>,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
