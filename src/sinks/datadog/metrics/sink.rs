use std::fmt;

use async_trait::async_trait;
use futures_util::{
    future::ready,
    stream::{self, BoxStream},
    StreamExt,
};
use tower::Service;
use vector_core::{
    buffers::Acker,
    event::{Event, Metric, MetricValue},
    partition::Partitioner,
    sink::StreamSink,
    stream::{BatcherSettings, DriverResponse},
};

use super::{
    config::DatadogMetricsEndpoint, normalizer::DatadogMetricsNormalizer,
    request_builder::DatadogMetricsRequestBuilder, service::DatadogMetricsRequest,
};
use crate::{
    config::SinkContext, internal_events::DatadogMetricsEncodingError, sinks::util::SinkBuilderExt,
};

/// Partitions metrics based on which Datadog API endpoint that they are sent to.
///
/// Generally speaking, all "basic" metrics -- counter, gauge, set, aggregated summary-- are sent to
/// the Series API, while distributions, aggregated histograms, and sketches (hehe) are sent to the
/// Sketches API.
struct DatadogMetricsTypePartitioner;

impl Partitioner for DatadogMetricsTypePartitioner {
    type Item = Metric;
    type Key = DatadogMetricsEndpoint;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        match item.data().value() {
            MetricValue::Counter { .. } => DatadogMetricsEndpoint::Series,
            MetricValue::Gauge { .. } => DatadogMetricsEndpoint::Series,
            MetricValue::Set { .. } => DatadogMetricsEndpoint::Series,
            MetricValue::Distribution { .. } => DatadogMetricsEndpoint::Sketches,
            MetricValue::AggregatedHistogram { .. } => DatadogMetricsEndpoint::Sketches,
            MetricValue::AggregatedSummary { .. } => DatadogMetricsEndpoint::Series,
            MetricValue::Sketch { .. } => DatadogMetricsEndpoint::Sketches,
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
    S::Error: fmt::Debug + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse,
{
    /// Creates a new `DatadogMetricsSink`.
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
        let sink = input
            // Convert `Event` to `Metric` so we don't have to deal with constant conversions.
            .filter_map(|event| ready(event.try_into_metric()))
            // Converts "absolute" metrics to "incremental", and converts distributions and
            // aggregated histograms into sketches so that we can send them in a more DD-native
            // format and thus avoid needing to directly specify what quantiles to generate, etc.
            .normalized_with_default::<DatadogMetricsNormalizer>()
            // We batch metrics by their endpoint i.e. series (counter, gauge, set) vs sketch
            // (distributions, aggregated histograms, metrics that are already sketches)
            .batched_partitioned(DatadogMetricsTypePartitioner, self.batch_settings)
            // We build our requests "incrementally", which means that for a single batch of
            // metrics, we might generate N requests to send them all, as Datadog has API-level
            // limits on payload size, so we keep adding metrics to a request until we reach the
            // limit, and then create a new request, and so on and so forth, until all metrics have
            // been turned into a request.
            .incremental_request_builder(self.request_builder)
            // This unrolls the vector of request results that our request builder generates.
            .flat_map(stream::iter)
            // Generating requests _can_ fail, so we log and filter out errors here.
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        let (error, dropped_events) = e.into_parts();
                        emit!(&DatadogMetricsEncodingError {
                            error,
                            dropped_events,
                        });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            // Finally, we generate the driver which will take our requests, send them off, and
            // appropriately handle finalization of the events, acking for buffers, and
            // logging/metrics, as the requests are responded to.
            .into_driver(self.service, self.acker);

        sink.run().await
    }
}

#[async_trait]
impl<S> StreamSink<Event> for DatadogMetricsSink<S>
where
    S: Service<DatadogMetricsRequest> + Send,
    S::Error: fmt::Debug + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        // Rust has issues with lifetimes and generics, which `async_trait` exacerbates, so we write
        // a normal async fn in `DatadogMetricsSink` itself, and then call out to it from this trait
        // implementation, which makes the compiler happy.
        self.run_inner(input).await
    }
}
