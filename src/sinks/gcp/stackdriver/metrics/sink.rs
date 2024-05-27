use vector_lib::event::{Metric, MetricValue};

use crate::sinks::{
    prelude::*,
    util::{
        buffer::metrics::{MetricNormalize, MetricSet},
        http::HttpRequest,
    },
};

use super::request_builder::StackdriverMetricsRequestBuilder;

#[derive(Clone, Debug, Default)]
struct StackdriverMetricsNormalize;

impl MetricNormalize for StackdriverMetricsNormalize {
    fn normalize(&mut self, state: &mut MetricSet, metric: Metric) -> Option<Metric> {
        match (metric.kind(), &metric.value()) {
            (_, MetricValue::Counter { .. }) => state.make_absolute(metric),
            (_, MetricValue::Gauge { .. }) => state.make_absolute(metric),
            // All others are left as-is
            _ => Some(metric),
        }
    }
}

pub(super) struct StackdriverMetricsSink<S> {
    service: S,
    batch_settings: BatcherSettings,
    request_builder: StackdriverMetricsRequestBuilder,
}

impl<S> StackdriverMetricsSink<S>
where
    S: Service<HttpRequest<()>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    /// Creates a new `StackdriverMetricsSink`.
    pub(super) const fn new(
        service: S,
        batch_settings: BatcherSettings,
        request_builder: StackdriverMetricsRequestBuilder,
    ) -> Self {
        Self {
            service,
            batch_settings,
            request_builder,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            .filter_map(|event| {
                // Filter out anything that is not a Counter or a Gauge.
                let metric = event.into_metric();

                future::ready(match metric.value() {
                    &MetricValue::Counter { .. } => Some(metric),
                    &MetricValue::Gauge { .. } => Some(metric),
                    not_supported => {
                        warn!("Unsupported metric type: {:?}.", not_supported);
                        None
                    }
                })
            })
            .normalized_with_default::<StackdriverMetricsNormalize>()
            .batched(self.batch_settings.as_byte_size_config())
            .request_builder(
                default_request_builder_concurrency_limit(),
                self.request_builder,
            )
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
            .run()
            .await
    }
}

#[async_trait::async_trait]
impl<S> StreamSink<Event> for StackdriverMetricsSink<S>
where
    S: Service<HttpRequest<()>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(
        self: Box<Self>,
        input: futures_util::stream::BoxStream<'_, Event>,
    ) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
