use std::{fmt, sync::Arc};

use async_trait::async_trait;
use chrono::Utc;
use futures_util::{
    future::ready,
    stream::{self, BoxStream},
    StreamExt,
};
use tower::Service;
use vector_common::finalization::EventFinalizers;
use vector_core::{
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
    internal_events::DatadogMetricsEncodingError,
    sinks::util::{
        buffer::metrics::sort::sort_for_compression,
        buffer::metrics::{AggregatedSummarySplitter, MetricSplitter},
        SinkBuilderExt,
    },
};

/// Partitions metrics based on which Datadog API endpoint that they are sent to.
///
/// Generally speaking, all "basic" metrics -- counter, gauge, set, aggregated summary-- are sent to
/// the Series API, while distributions, aggregated histograms, and sketches (hehe) are sent to the
/// Sketches API.
struct DatadogMetricsTypePartitioner;

impl Partitioner for DatadogMetricsTypePartitioner {
    type Item = Metric;
    type Key = (Option<Arc<str>>, DatadogMetricsEndpoint);

    fn partition(&self, item: &Self::Item) -> Self::Key {
        let endpoint = match item.data().value() {
            MetricValue::Counter { .. } => DatadogMetricsEndpoint::Series,
            MetricValue::Gauge { .. } => DatadogMetricsEndpoint::Series,
            MetricValue::Set { .. } => DatadogMetricsEndpoint::Series,
            MetricValue::Distribution { .. } => DatadogMetricsEndpoint::Sketches,
            MetricValue::AggregatedHistogram { .. } => DatadogMetricsEndpoint::Sketches,
            MetricValue::AggregatedSummary { .. } => DatadogMetricsEndpoint::Series,
            MetricValue::Sketch { .. } => DatadogMetricsEndpoint::Sketches,
        };
        (item.metadata().datadog_api_key(), endpoint)
    }
}

pub(crate) struct DatadogMetricsSink<S> {
    service: S,
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
    pub const fn new(
        service: S,
        request_builder: DatadogMetricsRequestBuilder,
        batch_settings: BatcherSettings,
    ) -> Self {
        DatadogMetricsSink {
            service,
            request_builder,
            batch_settings,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let mut splitter: MetricSplitter<AggregatedSummarySplitter> = MetricSplitter::default();

        let sink = input
            // Convert `Event` to `Metric` so we don't have to deal with constant conversions.
            .filter_map(|event| ready(event.try_into_metric()))
            // Split aggregated summaries into individual metrics for count, sum, and the quantiles, which lets us
            // ensure that aggregated summaries effectively make it through normalization, as we can't actually
            // normalize them and so they would be dropped during normalization otherwise.
            .flat_map(|metric| stream::iter(splitter.split(metric)))
            // Converts "absolute" metrics to "incremental", and converts distributions and aggregated histograms into
            // sketches so that we can send them in a more DD-native format and thus avoid needing to directly specify
            // what quantiles to generate, etc.
            .normalized_with_default::<DatadogMetricsNormalizer>()
            // We batch metrics by their endpoint: series endpoint for counters, gauge, and sets vs sketch endpoint for
            // distributions, aggregated histograms, and sketches.
            .batched_partitioned(DatadogMetricsTypePartitioner, self.batch_settings)
            // Aggregate counters with identical timestamps, otherwise identical counters (same
            // series and same timestamp, when rounded to whole seconds) will be dropped in a
            // last-write-wins situation when they hit the DD metrics intake.
            .map(|((api_key, endpoint), metrics)| {
                let collapsed_metrics = collapse_counters_by_series_and_timestamp(metrics);
                ((api_key, endpoint), collapsed_metrics)
            })
            // Sort metrics by name, which significantly improves HTTP compression.
            .map(|((api_key, endpoint), mut metrics)| {
                sort_for_compression(&mut metrics);
                ((api_key, endpoint), metrics)
            })
            // We build our requests "incrementally", which means that for a single batch of metrics, we might generate
            // N requests to send them all, as Datadog has API-level limits on payload size, so we keep adding metrics
            // to a request until we reach the limit, and then create a new request, and so on and so forth, until all
            // metrics have been turned into a request.
            .incremental_request_builder(self.request_builder)
            // This unrolls the vector of request results that our request builder generates.
            .flat_map(stream::iter)
            // Generating requests _can_ fail, so we log and filter out errors here.
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        let (message, error_code, dropped_events) = e.into_parts();
                        emit!(DatadogMetricsEncodingError {
                            message,
                            error_code,
                            dropped_events,
                        });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            // Finally, we generate the driver which will take our requests, send them off, and appropriately handle
            // finalization of the events, and logging/metrics, as the requests are responded to.
            .into_driver(self.service);

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

fn collapse_counters_by_series_and_timestamp(mut metrics: Vec<Metric>) -> Vec<Metric> {
    let mut remaining = metrics.len();
    let mut idx = 0;
    let now_ts = Utc::now().timestamp();

    // For each metric, see if it's a counter. If so, we check the rest of the metrics
    // _after_ it to see if they share the same series _and_ timestamp, when converted
    // to a Unix timestamp. If they match, we take that counter's value and merge it
    // with our "current" counter metric, and then drop the secondary one from the
    // vector.
    //
    // For any non-counter, we simply ignore it and leave it as-is.
    while remaining > 1 {
        let curr_idx = idx;
        let counter_ts = match metrics[curr_idx].value() {
            MetricValue::Counter { .. } => metrics[curr_idx]
                .data()
                .timestamp()
                .map(|dt| dt.timestamp())
                .unwrap_or(now_ts),
            // If it's not a counter, we can skip it.
            _ => {
                remaining -= 1;
                idx += 1;
                continue;
            }
        };

        let mut accumulated_value = 0.0;
        let mut accumulated_finalizers = EventFinalizers::default();

        // Now go through each metric _after_ the current one to see if it matches the
        // current metric: is a counter, with the same name and timestamp. If it is, we
        // accumulate its value and then remove it.
        //
        // Otherwise, we skip it.
        let mut is_disjoint = false;
        let mut had_match = false;
        let mut inner_idx = curr_idx + 1;
        let mut inner_remaining = remaining;
        while inner_remaining > 0 {
            match metrics[inner_idx].value() {
                MetricValue::Counter { value } => {
                    let other_counter_ts = metrics[inner_idx]
                        .data()
                        .timestamp()
                        .map(|dt| dt.timestamp())
                        .unwrap_or(now_ts);
                    if metrics[curr_idx].series() == metrics[inner_idx].series()
                        && counter_ts == other_counter_ts
                    {
                        had_match = true;

                        // Collapse this counter by accumulating its value, and its
                        // finalizers, and removing it from the original vector of metrics.
                        accumulated_value += *value;

                        let mut old_metric = metrics.swap_remove(inner_idx);
                        accumulated_finalizers.merge(old_metric.metadata_mut().take_finalizers());
                    } else {
                        // We hit a counter that _doesn't_ match, but we can't just skip
                        // it because we also need to evaulate it against all the
                        // counters that come after it, so we only increment the index
                        // for this inner loop.
                        //
                        // As well, we mark ourselves to stop incrementing the outer
                        // index if we find more counters to accumulate, because we've
                        // hit a disjoint counter here. While we may be continuing to
                        // shrink the count of remaining metrics from accumulating,
                        // we have to ensure this counter we just visited is visited by
                        // the outer loop.
                        is_disjoint = true;
                    }
                }
                // If it's not a counter, we can skip it.
                _ => {}
            };

            // Update our indexes.
            inner_idx += 1;
            inner_remaining -= 1;
            remaining -= 1;

            if !is_disjoint {
                idx += 1;
            }
        }

        // If we had matches during the accumulator phase, update our original counter.
        if had_match {
            match metrics.get_mut(curr_idx) {
                Some(metric) => match metric.value_mut() {
                    MetricValue::Counter { value } => {
                        *value += accumulated_value;
                        metric
                            .metadata_mut()
                            .merge_finalizers(accumulated_finalizers);
                    }
                    _ => unreachable!("current index must represent a counter"),
                },
                _ => unreachable!("current index must exist"),
            }
        }

        remaining -= 1;
        idx += 1;
    }
}
