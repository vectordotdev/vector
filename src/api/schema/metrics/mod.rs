mod errors;
pub mod filter;
mod host;
mod processed_bytes;
mod processed_events;
mod sink;
pub mod source;
mod transform;
mod uptime;

use async_graphql::{validators::IntRange, Interface, Object, Subscription};
use chrono::{DateTime, Utc};
use tokio::stream::{Stream, StreamExt};

pub use errors::{ComponentErrorsTotal, ErrorsTotal};
pub use filter::*;
pub use host::HostMetrics;
pub use processed_bytes::{
    ComponentProcessedBytesThroughput, ComponentProcessedBytesTotal, ProcessedBytesTotal,
};
pub use processed_events::{
    ComponentProcessedEventsThroughput, ComponentProcessedEventsTotal, ProcessedEventsTotal,
};
pub use sink::{IntoSinkMetrics, SinkMetrics};
pub use source::{IntoSourceMetrics, SourceMetrics};
pub use transform::{IntoTransformMetrics, TransformMetrics};
pub use uptime::Uptime;

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
        get_metrics(interval).filter_map(|m| match m.name() {
            "uptime_seconds" => Some(Uptime::new(m)),
            _ => None,
        })
    }

    /// Event processing metrics.
    async fn processed_events_total(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "10", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = ProcessedEventsTotal> {
        get_metrics(interval).filter_map(|m| match m.name() {
            "processed_events_total" => Some(ProcessedEventsTotal::new(m)),
            _ => None,
        })
    }

    /// Event processing throughput sampled over the provided millisecond `interval`.
    async fn processed_events_throughput(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "10", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = i64> {
        counter_throughput(interval, &|m| m.name() == "processed_events_total")
            .map(|(_, throughput)| throughput as i64)
    }

    /// Component event processing throughput metrics over `interval`.
    async fn component_processed_events_throughputs(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "10", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentProcessedEventsThroughput>> {
        component_counter_throughputs(interval, &|m| m.name() == "processed_events_total").map(
            |m| {
                m.into_iter()
                    .map(|(m, throughput)| {
                        ComponentProcessedEventsThroughput::new(
                            m.tag_value("component_name").unwrap(),
                            throughput as i64,
                        )
                    })
                    .collect()
            },
        )
    }

    /// Component event processing metrics over `interval`.
    async fn component_processed_events_totals(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "10", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentProcessedEventsTotal>> {
        component_counter_metrics(interval, &|m| m.name() == "processed_events_total").map(|m| {
            m.into_iter()
                .map(ComponentProcessedEventsTotal::new)
                .collect()
        })
    }

    /// Byte processing metrics.
    async fn processed_bytes_total(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "10", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = ProcessedBytesTotal> {
        get_metrics(interval).filter_map(|m| match m.name() {
            "processed_bytes_total" => Some(ProcessedBytesTotal::new(m)),
            _ => None,
        })
    }

    /// Byte processing throughput sampled over a provided millisecond `interval`.
    async fn processed_bytes_throughput(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "10", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = i64> {
        counter_throughput(interval, &|m| m.name() == "processed_bytes_total")
            .map(|(_, throughput)| throughput as i64)
    }

    /// Component byte processing metrics over `interval`.
    async fn component_processed_bytes_totals(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "10", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentProcessedBytesTotal>> {
        component_counter_metrics(interval, &|m| m.name() == "processed_bytes_total").map(|m| {
            m.into_iter()
                .map(ComponentProcessedBytesTotal::new)
                .collect()
        })
    }

    /// Component byte processing throughput over `interval`
    async fn component_processed_bytes_throughputs(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "10", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentProcessedBytesThroughput>> {
        component_counter_throughputs(interval, &|m| m.name() == "processed_bytes_total").map(|m| {
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
            .filter(|m| m.name().ends_with("_errors_total"))
            .map(ErrorsTotal::new)
    }

    /// Component error metrics over `interval`.
    async fn component_errors_totals(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "10", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentErrorsTotal>> {
        component_counter_metrics(interval, &|m| m.name().ends_with("_errors_total"))
            .map(|m| m.into_iter().map(ComponentErrorsTotal::new).collect())
    }

    /// All metrics.
    async fn metrics(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "10", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = MetricType> {
        get_metrics(interval).filter_map(|m| match m.name() {
            "uptime_seconds" => Some(MetricType::Uptime(m.into())),
            "processed_events_total" => Some(MetricType::ProcessedEventsTotal(m.into())),
            "processed_bytes_total" => Some(MetricType::ProcessedBytesTotal(m.into())),
            _ => None,
        })
    }
}
