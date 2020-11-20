mod errors;
mod host;
mod processed_bytes;
mod processed_events;
mod uptime;

use super::components::{self, Component, COMPONENTS};
use crate::{
    event::{Event, Metric, MetricValue},
    metrics::{capture_metrics, get_controller, Controller},
};
use async_graphql::{validators::IntRange, Interface, Object, Subscription};
use async_stream::stream;
use chrono::{DateTime, Utc};
use itertools::Itertools;
use lazy_static::lazy_static;
use std::{collections::BTreeMap, sync::Arc};
use tokio::{
    stream::{Stream, StreamExt},
    time::Duration,
};

pub use errors::{ComponentErrorsTotal, ErrorsTotal};
pub use host::HostMetrics;
pub use processed_bytes::{
    ComponentProcessedBytesThroughput, ComponentProcessedBytesTotal, ProcessedBytesTotal,
};
pub use processed_events::{
    ComponentProcessedEventsThroughput, ComponentProcessedEventsTotal, ProcessedEventsTotal,
};
pub use uptime::Uptime;

lazy_static! {
    static ref GLOBAL_CONTROLLER: Arc<&'static Controller> =
        Arc::new(get_controller().expect("Metrics system not initialized. Please report."));
}

#[derive(Interface)]
#[graphql(field(name = "timestamp", type = "Option<DateTime<Utc>>"))]
pub enum MetricType {
    Uptime(Uptime),
    ProcessedEventsTotal(ProcessedEventsTotal),
    ProcessedBytesTotal(ProcessedBytesTotal),
}

#[derive(Default)]
pub struct MetricsQuery;

#[Object]
impl MetricsQuery {
    /// Vector host metrics
    async fn host_metrics(&self) -> HostMetrics {
        HostMetrics::new()
    }
}

#[derive(Default)]
pub struct MetricsSubscription;

#[Subscription]
impl MetricsSubscription {
    /// Metrics for how long the Vector instance has been running.
    async fn uptime(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "10", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = Uptime> {
        get_metrics(interval).filter_map(|m| match m.name.as_str() {
            "uptime_seconds" => Some(Uptime::new(m)),
            _ => None,
        })
    }

    /// Events processed metrics.
    async fn processed_events_total(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "10", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = ProcessedEventsTotal> {
        get_metrics(interval).filter_map(|m| match m.name.as_str() {
            "processed_events_total" => Some(ProcessedEventsTotal::new(m)),
            _ => None,
        })
    }

    /// Events processed throughput, sampled over a provided millisecond `interval`.
    async fn processed_events_throughput(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "10", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = i64> {
        counter_throughput(interval, &|m| m.name == "processed_events_total")
            .map(|(_, throughput)| throughput as i64)
    }

    /// Component events processed throughputs over `interval`.
    async fn component_processed_events_throughputs(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "10", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentProcessedEventsThroughput>> {
        component_counter_throughputs(interval, &|m| m.name == "processed_events_total").map(|m| {
            m.into_iter()
                .map(|(m, throughput)| {
                    ComponentProcessedEventsThroughput::new(
                        m.tag_value("component_name").unwrap(),
                        throughput as i64,
                    )
                })
                .collect()
        })
    }

    /// Component events processed metrics over `interval`.
    async fn component_processed_events_totals(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "10", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentProcessedEventsTotal>> {
        component_counter_metrics(interval, &|m| m.name == "processed_events_total").map(|m| {
            m.into_iter()
                .map(ComponentProcessedEventsTotal::new)
                .collect()
        })
    }

    /// Bytes processed metrics.
    async fn processed_bytes_total(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "10", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = ProcessedBytesTotal> {
        get_metrics(interval).filter_map(|m| match m.name.as_str() {
            "processed_bytes_total" => Some(ProcessedBytesTotal::new(m)),
            _ => None,
        })
    }

    /// Bytes processed throughput, sampled over a provided millisecond `interval`.
    async fn processed_bytes_throughput(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "10", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = i64> {
        counter_throughput(interval, &|m| m.name == "processed_bytes_total")
            .map(|(_, throughput)| throughput as i64)
    }

    /// Component bytes processed metrics, over `interval`.
    async fn component_processed_bytes_totals(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "10", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentProcessedBytesTotal>> {
        component_counter_metrics(interval, &|m| m.name == "processed_bytes_total").map(|m| {
            m.into_iter()
                .map(ComponentProcessedBytesTotal::new)
                .collect()
        })
    }

    /// Component bytes processed throughputs, over `interval`
    async fn component_processed_bytes_throughputs(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "10", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentProcessedBytesThroughput>> {
        component_counter_throughputs(interval, &|m| m.name == "processed_bytes_total").map(|m| {
            m.into_iter()
                .map(|(m, throughput)| {
                    ComponentProcessedBytesThroughput::new(
                        m.tag_value("component_name").unwrap(),
                        throughput as i64,
                    )
                })
                .collect()
        })
    }

    /// Total error metrics.
    async fn errors_total(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "10", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = ErrorsTotal> {
        get_metrics(interval)
            .filter(|m| m.name.ends_with("_errors_total"))
            .map(ErrorsTotal::new)
    }

    /// Component errors metrics, over `interval`.
    async fn component_errors_totals(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "10", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentErrorsTotal>> {
        component_counter_metrics(interval, &|m| m.name.ends_with("_errors_total"))
            .map(|m| m.into_iter().map(ComponentErrorsTotal::new).collect())
    }

    /// All metrics.
    async fn metrics(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "10", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = MetricType> {
        get_metrics(interval).filter_map(|m| match m.name.as_str() {
            "uptime_seconds" => Some(MetricType::Uptime(m.into())),
            "processed_events_total" => Some(MetricType::ProcessedEventsTotal(m.into())),
            "processed_bytes_total" => Some(MetricType::ProcessedBytesTotal(m.into())),
            _ => None,
        })
    }
}

/// Returns a stream of `Metric`s, collected at the provided millisecond interval.
fn get_metrics(interval: i32) -> impl Stream<Item = Metric> {
    let controller = get_controller().unwrap();
    let mut interval = tokio::time::interval(Duration::from_millis(interval as u64));

    stream! {
        loop {
            interval.tick().await;
            for ev in capture_metrics(&controller) {
                if let Event::Metric(m) = ev {
                    yield m;
                }
            }
        }
    }
}

/// Returns a stream of `Metrics`, sorted into source, transform and sinks, in that order.
/// Metrics are collected into a `Vec<Metric>`, yielding at `inverval` milliseconds.
fn metrics_sorted(interval: i32) -> impl Stream<Item = Vec<Metric>> {
    let controller = get_controller().unwrap();
    let mut interval = tokio::time::interval(Duration::from_millis(interval as u64));

    // Sort each interval of metrics by key
    stream! {
        loop {
            interval.tick().await;

            yield capture_metrics(&controller)
                .filter_map(|m| match m {
                    Event::Metric(m) => match m.tag_value("component_name") {
                        Some(name) => match COMPONENTS.read().expect(components::INVARIANT).get(&name) {
                            Some(t) => Some(match t {
                                Component::Source(_) => (m, 1),
                                Component::Transform(_) => (m, 2),
                                Component::Sink(_) => (m, 3),
                            }),
                            _ => None,
                        },
                        _ => None,
                    },
                    _ => None,
                })
                .sorted_by_key(|m| m.1)
                .map(|(m, _)| m)
                .collect();
        }
    }
}

/// Get the events processed by component name.
pub fn component_processed_events_total(component_name: &str) -> Option<ProcessedEventsTotal> {
    capture_metrics(&GLOBAL_CONTROLLER)
        .find(|ev| match ev {
            Event::Metric(m)
                if m.name.as_str().eq("processed_events_total")
                    && m.tag_matches("component_name", &component_name) =>
            {
                true
            }
            _ => false,
        })
        .map(|ev| ProcessedEventsTotal::new(ev.into_metric()))
}

/// Get the bytes processed by component name.
pub fn component_processed_bytes_total(component_name: &str) -> Option<ProcessedBytesTotal> {
    capture_metrics(&GLOBAL_CONTROLLER)
        .find(|ev| match ev {
            Event::Metric(m)
                if m.name.as_str().eq("processed_bytes_total")
                    && m.tag_matches("component_name", &component_name) =>
            {
                true
            }
            _ => false,
        })
        .map(|ev| ProcessedBytesTotal::new(ev.into_metric()))
}

type MetricFilterFn = dyn Fn(&Metric) -> bool + Send + Sync;

/// Returns a stream of `Vec<Metric>`, where `metric_name` matches the name of the metric
/// (e.g. "processed_events_total"), and the value is derived from `MetricValue::Counter`. Uses a
/// local cache to match against the `component_name` of a metric, to return results only when
/// the value of a current iteration is greater than the previous. This is useful for the client
/// to be notified as metrics increase without returning 'empty' or identical results.
pub fn component_counter_metrics(
    interval: i32,
    filter_fn: &'static MetricFilterFn,
) -> impl Stream<Item = Vec<Metric>> {
    let mut cache = BTreeMap::new();

    metrics_sorted(interval).map(move |m| {
        m.into_iter()
            .filter(filter_fn)
            .filter_map(|m| {
                let component_name = m.tag_value("component_name")?;
                match m.value {
                    MetricValue::Counter { value }
                        if cache.insert(component_name, value).unwrap_or(0.00) < value =>
                    {
                        Some(m)
                    }
                    _ => None,
                }
            })
            .collect()
    })
}

/// Returns the throughput of a 'counter' metric, sampled over `interval` millseconds
/// and filtered by the provided `filter_fn`.
fn counter_throughput(
    interval: i32,
    filter_fn: &'static MetricFilterFn,
) -> impl Stream<Item = (Metric, f64)> {
    let mut last = 0.00;

    get_metrics(interval)
        .filter(filter_fn)
        .filter_map(move |m| match m.value {
            MetricValue::Counter { value } if value > last => {
                let throughput = value - last;
                last = value;
                Some((m, throughput))
            }
            _ => None,
        })
        // Ignore the first, since we only care about sampling between `interval`
        .skip(1)
}

/// Returns the throughput of a 'counter' metric, sampled over `interval` milliseconds
/// and filtered by the provided `filter_fn`, aggregated against each component.
fn component_counter_throughputs(
    interval: i32,
    filter_fn: &'static MetricFilterFn,
) -> impl Stream<Item = Vec<(Metric, f64)>> {
    let mut cache = BTreeMap::new();

    metrics_sorted(interval)
        .map(move |m| {
            m.into_iter()
                .filter(filter_fn)
                .filter_map(|m| {
                    let component_name = m.tag_value("component_name")?;
                    match m.value {
                        MetricValue::Counter { value } => {
                            let last = cache.insert(component_name, value).unwrap_or(0.00);
                            let throughput = value - last;
                            Some((m, throughput))
                        }
                        _ => None,
                    }
                })
                .collect()
        })
        // Ignore the first, since we only care about sampling between `interval`
        .skip(1)
}
