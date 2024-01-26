use async_trait::async_trait;

use futures::StreamExt;
use futures_util::stream::BoxStream;
use vector_lib::event::{Metric, MetricValue};

use crate::sinks::prelude::*;
use crate::sinks::util::buffer::metrics::MetricNormalize;
use crate::sinks::util::buffer::metrics::MetricSet;

use super::batch::GreptimeDBBatchSizer;
use super::service::{GreptimeDBRequest, GreptimeDBRetryLogic, GreptimeDBService};

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

pub struct GreptimeDBSink {
    pub(super) service: Svc<GreptimeDBService, GreptimeDBRetryLogic>,
    pub(super) batch_settings: BatcherSettings,
}

impl GreptimeDBSink {
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            .map(|event| event.into_metric())
            .normalized_with_default::<GreptimeDBMetricNormalize>()
            .batched(
                self.batch_settings
                    .as_item_size_config(GreptimeDBBatchSizer),
            )
            .map(GreptimeDBRequest::from_metrics)
            .into_driver(self.service)
            .protocol("grpc")
            .run()
            .await
    }
}

#[async_trait]
impl StreamSink<Event> for GreptimeDBSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
