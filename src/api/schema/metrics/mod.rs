mod allocated_bytes;
mod errors;
pub mod filter;
mod output;
mod received_bytes;
mod received_events;
mod sent_bytes;
mod sent_events;
mod sink;
pub mod source;
mod transform;
mod uptime;

#[cfg(feature = "sources-host_metrics")]
mod host;

pub use allocated_bytes::{AllocatedBytes, ComponentAllocatedBytes};
use async_graphql::{Interface, Subscription};
use chrono::{DateTime, Utc};
pub use errors::{ComponentErrorsTotal, ErrorsTotal};
pub use filter::*;
pub use output::*;
pub use received_bytes::{
    ComponentReceivedBytesThroughput, ComponentReceivedBytesTotal, ReceivedBytesTotal,
};
pub use received_events::{
    ComponentReceivedEventsThroughput, ComponentReceivedEventsTotal, ReceivedEventsTotal,
};
pub use sent_bytes::{ComponentSentBytesThroughput, ComponentSentBytesTotal, SentBytesTotal};
pub use sent_events::{ComponentSentEventsThroughput, ComponentSentEventsTotal, SentEventsTotal};
pub use sink::{IntoSinkMetrics, SinkMetrics};
pub use source::{IntoSourceMetrics, SourceMetrics};
use tokio_stream::{Stream, StreamExt};
pub use transform::{IntoTransformMetrics, TransformMetrics};
pub use uptime::Uptime;

use crate::config::ComponentKey;

#[derive(Interface)]
#[graphql(field(name = "timestamp", ty = "Option<DateTime<Utc>>"))]
pub enum MetricType {
    Uptime(Uptime),
}

#[derive(Default)]
pub struct MetricsQuery;

#[cfg(feature = "sources-host_metrics")]
#[async_graphql::Object]
impl MetricsQuery {
    /// Vector host metrics
    async fn host_metrics(&self) -> host::HostMetrics {
        host::HostMetrics::new()
    }
}

#[derive(Default)]
pub struct MetricsSubscription;

#[Subscription]
impl MetricsSubscription {
    /// Metrics for how long the Vector instance has been running
    async fn uptime(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = Uptime> {
        get_metrics(interval).filter_map(|m| match m.name() {
            "uptime_seconds" => Some(Uptime::new(m)),
            _ => None,
        })
    }

    /// Total received events metrics
    #[graphql(deprecation = "Use component_received_events_totals instead")]
    async fn received_events_total(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = ReceivedEventsTotal> {
        get_metrics(interval).filter_map(|m| match m.name() {
            "component_received_events_total" => Some(ReceivedEventsTotal::new(m)),
            _ => None,
        })
    }

    /// Total received events throughput sampled over the provided millisecond `interval`
    #[graphql(deprecation = "Use component_received_events_throughputs instead")]
    async fn received_events_throughput(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = i64> {
        counter_throughput(interval, &|m| m.name() == "component_received_events_total")
            .map(|(_, throughput)| throughput as i64)
    }

    /// Total incoming component events throughput metrics over `interval`
    async fn component_received_events_throughputs(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentReceivedEventsThroughput>> {
        component_counter_throughputs(interval, &|m| m.name() == "component_received_events_total")
            .map(|m| {
                m.into_iter()
                    .map(|(m, throughput)| {
                        ComponentReceivedEventsThroughput::new(
                            ComponentKey::from(m.tag_value("component_id").unwrap()),
                            throughput as i64,
                        )
                    })
                    .collect()
            })
    }

    /// Total received component event metrics over `interval`
    async fn component_received_events_totals(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentReceivedEventsTotal>> {
        component_counter_metrics(interval, &|m| m.name() == "component_received_events_total").map(
            |m| {
                m.into_iter()
                    .map(ComponentReceivedEventsTotal::new)
                    .collect()
            },
        )
    }

    /// Total sent events metrics
    #[graphql(deprecation = "Use component_sent_events_totals instead")]
    async fn sent_events_total(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = SentEventsTotal> {
        get_metrics(interval).filter_map(|m| match m.name() {
            "component_sent_events_total" => Some(SentEventsTotal::new(m)),
            _ => None,
        })
    }

    /// Total outgoing events throughput sampled over the provided millisecond `interval`
    #[graphql(deprecation = "Use component_sent_events_throughputs instead")]
    async fn sent_events_throughput(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = i64> {
        counter_throughput(interval, &|m| m.name() == "component_sent_events_total")
            .map(|(_, throughput)| throughput as i64)
    }

    /// Total outgoing component event throughput metrics over `interval`
    async fn component_sent_events_throughputs(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentSentEventsThroughput>> {
        component_sent_events_total_throughputs_with_outputs(interval).map(|m| {
            m.into_iter()
                .map(|(key, total_throughput, outputs)| {
                    ComponentSentEventsThroughput::new(key, total_throughput, outputs)
                })
                .collect()
        })
    }

    /// Total outgoing component event metrics over `interval`
    async fn component_sent_events_totals(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentSentEventsTotal>> {
        component_sent_events_totals_metrics_with_outputs(interval).map(|ms| {
            ms.into_iter()
                .map(|(m, m_by_outputs)| ComponentSentEventsTotal::new(m, m_by_outputs))
                .collect()
        })
    }

    /// Component bytes received metrics over `interval`.
    async fn component_received_bytes_totals(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentReceivedBytesTotal>> {
        component_counter_metrics(interval, &|m| m.name() == "component_received_bytes_total").map(
            |m| {
                m.into_iter()
                    .map(ComponentReceivedBytesTotal::new)
                    .collect()
            },
        )
    }

    /// Component bytes received throughput over `interval`
    async fn component_received_bytes_throughputs(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentReceivedBytesThroughput>> {
        component_counter_throughputs(interval, &|m| m.name() == "component_received_bytes_total")
            .map(|m| {
                m.into_iter()
                    .map(|(m, throughput)| {
                        ComponentReceivedBytesThroughput::new(
                            ComponentKey::from(m.tag_value("component_id").unwrap()),
                            throughput as i64,
                        )
                    })
                    .collect()
            })
    }

    /// Component bytes sent metrics over `interval`.
    async fn component_sent_bytes_totals(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentSentBytesTotal>> {
        component_counter_metrics(interval, &|m| m.name() == "component_sent_bytes_total")
            .map(|m| m.into_iter().map(ComponentSentBytesTotal::new).collect())
    }

    /// Component bytes sent throughput over `interval`
    async fn component_sent_bytes_throughputs(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentSentBytesThroughput>> {
        component_counter_throughputs(interval, &|m| m.name() == "component_sent_bytes_total").map(
            |m| {
                m.into_iter()
                    .map(|(m, throughput)| {
                        ComponentSentBytesThroughput::new(
                            ComponentKey::from(m.tag_value("component_id").unwrap()),
                            throughput as i64,
                        )
                    })
                    .collect()
            },
        )
    }

    /// Total error metrics.
    async fn errors_total(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = ErrorsTotal> {
        get_metrics(interval)
            .filter(|m| m.name().ends_with("_errors_total"))
            .map(ErrorsTotal::new)
    }

    /// Allocated bytes metrics.
    async fn allocated_bytes(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = AllocatedBytes> {
        get_metrics(interval)
            .filter(|m| m.name() == "component_allocated_bytes")
            .map(AllocatedBytes::new)
    }

    /// Component allocation metrics
    async fn component_allocated_bytes(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentAllocatedBytes>> {
        component_gauge_metrics(interval, &|m| m.name() == "component_allocated_bytes")
            .map(|m| m.into_iter().map(ComponentAllocatedBytes::new).collect())
    }

    /// Component error metrics over `interval`.
    async fn component_errors_totals(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentErrorsTotal>> {
        component_counter_metrics(interval, &|m| m.name().ends_with("_errors_total"))
            .map(|m| m.into_iter().map(ComponentErrorsTotal::new).collect())
    }

    /// All metrics.
    async fn metrics(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = MetricType> {
        get_metrics(interval).filter_map(|m| match m.name() {
            "uptime_seconds" => Some(MetricType::Uptime(m.into())),
            _ => None,
        })
    }
}
