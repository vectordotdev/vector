use crate::sinks::{
    greptimedb::metrics::{
        batch::GreptimeDBBatchSizer, request::GreptimeDBGrpcRequest,
        request::GreptimeDBGrpcRetryLogic, request_builder::RequestBuilderOptions,
        service::GreptimeDBGrpcService,
    },
    prelude::*,
    util::buffer::metrics::{MetricNormalize, MetricSet},
};
use async_trait::async_trait;
use futures::StreamExt;
use futures_util::stream::BoxStream;
use vector_lib::event::{Metric, MetricValue};

#[derive(Clone, Debug, Default)]
pub struct GreptimeDBMetricNormalize;

impl MetricNormalize for GreptimeDBMetricNormalize {
    fn normalize(&mut self, state: &mut MetricSet, metric: Metric) -> Option<Metric> {
        match (metric.kind(), &metric.value()) {
            (_, MetricValue::Counter { .. }) => state.make_absolute(metric),
            (_, MetricValue::Gauge { .. }) => state.make_absolute(metric),
            // All others are left as-is
            _ => Some(metric),
        }
    }
}

/// GreptimeDBGrpcSink is a sink that sends metrics to GreptimeDB via gRPC.
/// It uses the `GreptimeDBGrpcService` to send the metrics.
pub struct GreptimeDBGrpcSink {
    pub(super) service: Svc<GreptimeDBGrpcService, GreptimeDBGrpcRetryLogic>,
    pub(super) batch_settings: BatcherSettings,
    pub(super) request_builder_options: RequestBuilderOptions,
}

impl GreptimeDBGrpcSink {
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            .map(|event| event.into_metric())
            .normalized_with_default::<GreptimeDBMetricNormalize>()
            .batched(
                self.batch_settings
                    .as_item_size_config(GreptimeDBBatchSizer),
            )
            .map(|m| GreptimeDBGrpcRequest::from_metrics(m, &self.request_builder_options))
            .into_driver(self.service)
            .protocol("grpc")
            .run()
            .await
    }
}

#[async_trait]
impl StreamSink<Event> for GreptimeDBGrpcSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
