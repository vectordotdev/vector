use std::{fmt, sync::Arc};

use async_trait::async_trait;
use chrono::Utc;
use futures_util::{
    future::ready,
    stream::{self, BoxStream},
    StreamExt,
};
use tower::Service;
use vector_lib::stream::{BatcherSettings, DriverResponse};
use vector_lib::{
    event::{Event, Metric, MetricValue},
    partition::Partitioner,
    sink::StreamSink,
};

use super::{
    config::DatadogMetricsEndpoint, normalizer::DatadogMetricsNormalizer,
    request_builder::DatadogMetricsRequestBuilder, service::DatadogMetricsRequest,
};
use crate::{
    internal_events::DatadogMetricsEncodingError,
    sinks::util::{
        buffer::metrics::{AggregatedSummarySplitter, MetricSplitter},
        request_builder::default_request_builder_concurrency_limit,
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
            MetricValue::Counter { .. } => DatadogMetricsEndpoint::series(),
            MetricValue::Gauge { .. } => DatadogMetricsEndpoint::series(),
            MetricValue::Set { .. } => DatadogMetricsEndpoint::series(),
            MetricValue::Distribution { .. } => DatadogMetricsEndpoint::Sketches,
            MetricValue::AggregatedHistogram { .. } => DatadogMetricsEndpoint::Sketches,
            // NOTE: AggregatedSummary will be split into counters and gauges during normalization
            MetricValue::AggregatedSummary { .. } => DatadogMetricsEndpoint::series(),
            MetricValue::Sketch { .. } => DatadogMetricsEndpoint::Sketches,
        };
        (item.metadata().datadog_api_key(), endpoint)
    }
}

pub(crate) struct DatadogMetricsSink<S> {
    service: S,
    request_builder: DatadogMetricsRequestBuilder,
    batch_settings: BatcherSettings,
    protocol: String,
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
        protocol: String,
    ) -> Self {
        DatadogMetricsSink {
            service,
            request_builder,
            batch_settings,
            protocol,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let mut splitter: MetricSplitter<AggregatedSummarySplitter> = MetricSplitter::default();
        let batch_settings = self.batch_settings;

        input
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
            .batched_partitioned(DatadogMetricsTypePartitioner, || {
                batch_settings.as_byte_size_config()
            })
            // Aggregate counters with identical timestamps, otherwise identical counters (same
            // series and same timestamp, when rounded to whole seconds) will be dropped in a
            // last-write-wins situation when they hit the DD metrics intake.
            //
            // This also sorts metrics by name, which significantly improves HTTP compression.
            .concurrent_map(
                default_request_builder_concurrency_limit(),
                |((api_key, endpoint), metrics)| {
                    Box::pin(async move {
                        let collapsed_metrics =
                            sort_and_collapse_counters_by_series_and_timestamp(metrics);
                        ((api_key, endpoint), collapsed_metrics)
                    })
                },
            )
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
                        let (reason, error_code, dropped_events) = e.into_parts();
                        emit!(DatadogMetricsEncodingError {
                            reason: reason.as_str(),
                            error_code,
                            dropped_events: dropped_events as usize,
                        });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            // Finally, we generate the driver which will take our requests, send them off, and appropriately handle
            // finalization of the events, and logging/metrics, as the requests are responded to.
            .into_driver(self.service)
            .protocol(self.protocol)
            .run()
            .await
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

/// Collapses counters by series and timestamp, leaving all other metrics unmodified.
/// The return value is sorted by metric series, which is desirable for compression. A sorted vector
/// tends to compress better than a random ordering by 2-3x (JSON encoded, deflate algorithm).
///
/// Note that the time complexity of this function is O(n log n) and the space complexity is O(1).
/// If needed, we can trade space for time by using a HashMap, which would be O(n) time and O(n) space.
fn sort_and_collapse_counters_by_series_and_timestamp(mut metrics: Vec<Metric>) -> Vec<Metric> {
    let now_ts = Utc::now().timestamp();

    // Sort by series and timestamp which is required for the below dedupe to behave as desired.
    // This also tends to compress better than a random ordering by 2-3x (JSON encoded, deflate algorithm).
    // Note that `sort_unstable_by_key` would be simpler but results in lifetime errors without cloning.
    metrics.sort_unstable_by(|a, b| {
        (
            a.value().as_name(),
            a.series(),
            a.timestamp().map(|dt| dt.timestamp()).unwrap_or(now_ts),
        )
            .cmp(&(
                a.value().as_name(),
                b.series(),
                b.timestamp().map(|dt| dt.timestamp()).unwrap_or(now_ts),
            ))
    });

    // Aggregate counters that share the same series and timestamp.
    // While `coalesce` is semantically more fitting here than `dedupe_by`, we opt for the latter because
    // they share the same functionality and `dedupe_by`'s implementation is more optimized, doing the
    // operation in place.
    metrics.dedup_by(|left, right| {
        if left.series() != right.series() {
            return false;
        }

        let left_ts = left.timestamp().map(|dt| dt.timestamp()).unwrap_or(now_ts);
        let right_ts = right.timestamp().map(|dt| dt.timestamp()).unwrap_or(now_ts);
        if left_ts != right_ts {
            return false;
        }

        // Only aggregate counters. All other types can be skipped.
        if let (
            MetricValue::Counter { value: left_value },
            MetricValue::Counter { value: right_value },
        ) = (left.value(), right.value_mut())
        {
            // NOTE: The docs for `dedup_by` specify that if `left`/`right` are equal, then
            // `left` is the element that gets removed.
            *right_value += left_value;
            right
                .metadata_mut()
                .merge_finalizers(left.metadata_mut().take_finalizers());

            true
        } else {
            false
        }
    });

    metrics
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, time::Duration};

    use chrono::{DateTime, Utc};
    use proptest::prelude::*;
    use vector_lib::{
        event::{Metric, MetricKind, MetricValue},
        metric_tags,
    };

    use super::sort_and_collapse_counters_by_series_and_timestamp;

    fn arb_collapsible_metrics() -> impl Strategy<Value = Vec<Metric>> {
        let ts = Utc::now();

        any::<Vec<(u16, MetricValue)>>().prop_map(move |values| {
            let mut unique_metrics = HashSet::new();
            values
                .into_iter()
                .map(|(id, value)| {
                    let name = format!("{}-{}", value.as_name(), id);
                    Metric::new(name, MetricKind::Incremental, value).with_timestamp(Some(ts))
                })
                // Filter out duplicates other than counters. We do this to prevent false positives. False positives would occur
                // because we don't collapse other metric types and we can't sort metrics by their values.
                .filter(|metric| {
                    matches!(metric.value(), MetricValue::Counter { .. })
                        || unique_metrics.insert(metric.series().clone())
                })
                .collect()
        })
    }

    fn create_counter(name: &str, value: f64) -> Metric {
        Metric::new(
            name,
            MetricKind::Incremental,
            MetricValue::Counter { value },
        )
    }

    fn create_gauge(name: &str, value: f64) -> Metric {
        Metric::new(name, MetricKind::Incremental, MetricValue::Gauge { value })
    }

    #[test]
    fn collapse_no_metrics() {
        let input = Vec::new();
        let expected = input.clone();
        let actual = sort_and_collapse_counters_by_series_and_timestamp(input);

        assert_eq!(expected, actual);
    }

    #[test]
    fn collapse_single_metric() {
        let input = vec![create_counter("basic", 42.0)];
        let expected = input.clone();
        let actual = sort_and_collapse_counters_by_series_and_timestamp(input);

        assert_eq!(expected, actual);
    }

    #[test]
    fn collapse_identical_metrics_gauge() {
        let input = vec![create_gauge("basic", 42.0), create_gauge("basic", 42.0)];
        let expected = input.clone();
        let actual = sort_and_collapse_counters_by_series_and_timestamp(input);

        assert_eq!(expected, actual);

        let gauge_value = 41.0;
        let input = vec![
            create_gauge("basic", gauge_value),
            create_gauge("basic", gauge_value),
            create_gauge("basic", gauge_value),
            create_gauge("basic", gauge_value),
            create_gauge("basic", gauge_value),
            create_gauge("basic", gauge_value),
            create_gauge("basic", gauge_value),
        ];
        let expected = input.clone();
        let actual = sort_and_collapse_counters_by_series_and_timestamp(input);

        assert_eq!(expected, actual);
    }

    #[test]
    fn collapse_identical_metrics_counter() {
        let counter_value = 42.0;
        let input = vec![
            create_counter("basic", counter_value),
            create_counter("basic", counter_value),
            create_counter("basic", counter_value),
            create_counter("basic", counter_value),
            create_counter("basic", counter_value),
            create_counter("basic", counter_value),
            create_counter("basic", counter_value),
        ];

        let expected_counter_value = input.len() as f64 * counter_value;
        let expected = vec![create_counter("basic", expected_counter_value)];
        let actual = sort_and_collapse_counters_by_series_and_timestamp(input);

        assert_eq!(expected, actual);
    }

    #[test]
    fn collapse_identical_metrics_counter_unsorted() {
        let gauge_value = 1.0;
        let counter_value = 42.0;
        let input = vec![
            create_gauge("gauge", gauge_value),
            create_counter("basic", counter_value),
            create_counter("basic", counter_value),
            create_counter("basic", counter_value),
            create_gauge("gauge", gauge_value),
            create_counter("basic", counter_value),
            create_counter("basic", counter_value),
            create_counter("basic", counter_value),
            create_counter("basic", counter_value),
        ];

        let expected_counter_value = (input.len() - 2) as f64 * counter_value;
        let expected = vec![
            create_counter("basic", expected_counter_value),
            create_gauge("gauge", gauge_value),
            create_gauge("gauge", gauge_value),
        ];
        let actual = sort_and_collapse_counters_by_series_and_timestamp(input);

        assert_eq!(expected, actual);
    }

    #[test]
    fn collapse_identical_metrics_multiple_timestamps() {
        let ts_1 = Utc::now() - Duration::from_secs(5);
        let ts_2 = ts_1 - Duration::from_secs(5);
        let counter_value = 42.0;
        let input = vec![
            create_counter("basic", counter_value),
            create_counter("basic", counter_value).with_timestamp(Some(ts_1)),
            create_counter("basic", counter_value).with_timestamp(Some(ts_2)),
            create_counter("basic", counter_value),
            create_counter("basic", counter_value).with_timestamp(Some(ts_2)),
            create_counter("basic", counter_value).with_timestamp(Some(ts_1)),
            create_counter("basic", counter_value),
        ];

        let expected = vec![
            create_counter("basic", counter_value * 2.).with_timestamp(Some(ts_2)),
            create_counter("basic", counter_value * 2.).with_timestamp(Some(ts_1)),
            create_counter("basic", counter_value * 3.),
        ];
        let actual = sort_and_collapse_counters_by_series_and_timestamp(input);

        assert_eq!(expected, actual);
    }

    #[test]
    fn collapse_identical_metrics_with_tags() {
        let counter_value = 42.0;
        let input = vec![
            create_counter("basic", counter_value).with_tags(Some(metric_tags!("a" => "a"))),
            create_counter("basic", counter_value).with_tags(Some(metric_tags!(
                "a" => "a",
                "b" => "b",
            ))),
            create_counter("basic", counter_value),
            create_counter("basic", counter_value).with_tags(Some(metric_tags!(
                "b" => "b",
                "a" => "a",
            ))),
            create_counter("basic", counter_value),
            create_counter("basic", counter_value),
            create_counter("basic", counter_value).with_tags(Some(metric_tags!("a" => "a"))),
        ];

        let expected = vec![
            create_counter("basic", counter_value * 3.),
            create_counter("basic", counter_value * 2.).with_tags(Some(metric_tags!("a" => "a"))),
            create_counter("basic", counter_value * 2.).with_tags(Some(metric_tags!(
                "a" => "a",
                "b" => "b",
            ))),
        ];
        let actual = sort_and_collapse_counters_by_series_and_timestamp(input);

        assert_eq!(expected, actual);
    }

    #[derive(Eq, Ord, PartialEq, PartialOrd)]
    struct MetricCollapseSort {
        metric_type: &'static str,
        metric_name: String,
        metric_ts: Option<DateTime<Utc>>,
    }

    impl MetricCollapseSort {
        fn from_metric(metric: &Metric) -> Self {
            Self {
                metric_type: metric.value().as_name(),
                metric_name: metric.name().to_string(),
                metric_ts: metric.timestamp(),
            }
        }
    }

    fn collapse_dedup_fn(left: &mut Metric, right: &mut Metric) -> bool {
        let series_eq = left.series() == right.series();
        let timestamp_eq = left.timestamp() == right.timestamp();
        if !series_eq || !timestamp_eq {
            return false;
        }

        match (left.value_mut(), right.value_mut()) {
            (
                MetricValue::Counter { value: left_value },
                MetricValue::Counter { value: right_value },
            ) => {
                // NOTE: The docs for `dedup_by` specify that if `left`/`right` are equal, then
                // `left` is the element that gets removed.
                *right_value += *left_value;
                true
            }
            // Only counters can be equivalent for the purpose of this test.
            _ => false,
        }
    }

    proptest! {
        #[test]
        fn test_counter_collapse(input in arb_collapsible_metrics()) {
            let mut expected_output = input.clone();
            expected_output.sort_by_cached_key(MetricCollapseSort::from_metric);
            expected_output.dedup_by(collapse_dedup_fn);

            let actual_output = sort_and_collapse_counters_by_series_and_timestamp(input);

            prop_assert_eq!(expected_output, actual_output);
        }
    }
}
