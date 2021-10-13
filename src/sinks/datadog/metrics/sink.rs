use async_trait::async_trait;
use futures_util::{StreamExt, future::ready, stream::BoxStream};
use http::Uri;
use tower::Service;
use vector_core::{buffers::Acker, event::{Event, Metric, MetricValue}, partition::Partitioner, sink::StreamSink, stream::BatcherSettings};

use crate::{
    config::SinkContext,
    sinks::util::{Compression, SinkBuilderExt},
};

use super::{config::DatadogMetricsEndpoint, service::DatadogMetricsRequest};

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
    metric_endpoints: Vec<(DatadogMetricsEndpoint, Uri)>,
    compression: Compression,
    batch_settings: BatcherSettings,
}

impl<S> DatadogMetricsSink<S>
where
    S: Service<DatadogMetricsRequest> + Send,
{
    pub fn new(
        cx: SinkContext,
        service: S,
        metric_endpoints: Vec<(DatadogMetricsEndpoint, Uri)>,
        compression: Compression,
        batch_settings: BatcherSettings,
    ) -> Self {
        DatadogMetricsSink {
            service,
            acker: cx.acker(),
            metric_endpoints,
            compression,
            batch_settings,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let sink = input
            .filter_map(|event| ready(event.try_into_metric()))
            .batched(DatadogMetricsTypePartitioner, self.batch_settings);
        //.into_driver(self.service, self.acker);

        //sink.run().await
        Ok(())
    }
}

#[async_trait]
impl<S> StreamSink for DatadogMetricsSink<S>
where
    S: Service<DatadogMetricsRequest> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
