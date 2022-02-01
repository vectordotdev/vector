mod errors;
mod events_in;
mod events_out;
pub mod filter;
mod output;
mod processed_bytes;
mod processed_events;
mod received_events;
mod sent_events;
mod sink;
pub mod source;
mod transform;
mod uptime;

#[cfg(feature = "sources-host_metrics")]
mod host;

use async_graphql::{Interface, Object, Subscription};
use chrono::{DateTime, Utc};
pub use errors::{ComponentErrorsTotal, ErrorsTotal};
pub use events_in::{ComponentEventsInThroughput, ComponentEventsInTotal, EventsInTotal};
pub use events_out::{ComponentEventsOutThroughput, ComponentEventsOutTotal, EventsOutTotal};
pub use filter::*;
pub use output::*;
pub use processed_bytes::{
    ComponentProcessedBytesThroughput, ComponentProcessedBytesTotal, ProcessedBytesTotal,
};
pub use processed_events::{
    ComponentProcessedEventsThroughput, ComponentProcessedEventsTotal, ProcessedEventsTotal,
};
pub use received_events::{
    ComponentReceivedEventsThroughput, ComponentReceivedEventsTotal, ReceivedEventsTotal,
};
pub use sent_events::{ComponentSentEventsThroughput, ComponentSentEventsTotal, SentEventsTotal};
pub use sink::{IntoSinkMetrics, SinkMetrics};
pub use source::{IntoSourceMetrics, SourceMetrics};
use tokio_stream::{Stream, StreamExt};
pub use transform::{IntoTransformMetrics, TransformMetrics};
pub use uptime::Uptime;

use crate::config::ComponentKey;

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
    #[cfg(feature = "sources-host_metrics")]
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

    /// Event processing metrics.
    async fn processed_events_total(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = ProcessedEventsTotal> {
        get_metrics(interval).filter_map(|m| match m.name() {
            "processed_events_total" => Some(ProcessedEventsTotal::new(m)),
            _ => None,
        })
    }

    /// Event processing throughput sampled over the provided millisecond `interval`.
    async fn processed_events_throughput(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = i64> {
        counter_throughput(interval, &|m| m.name() == "processed_events_total")
            .map(|(_, throughput)| throughput as i64)
    }

    /// Component event processing throughput metrics over `interval`.
    async fn component_processed_events_throughputs(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentProcessedEventsThroughput>> {
        component_counter_throughputs(interval, &|m| m.name() == "processed_events_total").map(
            |m| {
                m.into_iter()
                    .map(|(m, throughput)| {
                        ComponentProcessedEventsThroughput::new(
                            ComponentKey::from(m.tag_value("component_id").unwrap()),
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
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentProcessedEventsTotal>> {
        component_counter_metrics(interval, &|m| m.name() == "processed_events_total").map(|m| {
            m.into_iter()
                .map(ComponentProcessedEventsTotal::new)
                .collect()
        })
    }

    /// Total incoming events metrics
    #[graphql(deprecation = "Use received_events_total instead")]
    async fn events_in_total(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = EventsInTotal> {
        get_metrics(interval).filter_map(|m| match m.name() {
            "events_in_total" => Some(EventsInTotal::new(m)),
            _ => None,
        })
    }

    /// Total received events metrics
    async fn received_events_total(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = ReceivedEventsTotal> {
        get_metrics(interval).filter_map(|m| match m.name() {
            "component_received_events_total" => Some(ReceivedEventsTotal::new(m)),
            _ => None,
        })
    }

    /// Total incoming events throughput sampled over the provided millisecond `interval`
    #[graphql(deprecation = "Use received_events_throughput instead")]
    async fn events_in_throughput(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = i64> {
        counter_throughput(interval, &|m| m.name() == "events_in_total")
            .map(|(_, throughput)| throughput as i64)
    }

    /// Total received events throughput sampled over the provided millisecond `interval`
    async fn received_events_throughput(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = i64> {
        counter_throughput(interval, &|m| m.name() == "component_received_events_total")
            .map(|(_, throughput)| throughput as i64)
    }

    /// Total incoming component events throughput metrics over `interval`
    #[graphql(deprecation = "Use component_received_events_throughputs instead")]
    async fn component_events_in_throughputs(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentEventsInThroughput>> {
        component_counter_throughputs(interval, &|m| m.name() == "events_in_total").map(|m| {
            m.into_iter()
                .map(|(m, throughput)| {
                    ComponentEventsInThroughput::new(
                        ComponentKey::from(m.tag_value("component_id").unwrap()),
                        throughput as i64,
                    )
                })
                .collect()
        })
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

    /// Total incoming component event metrics over `interval`
    #[graphql(deprecation = "Use component_received_events_totals instead")]
    async fn component_events_in_totals(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentEventsInTotal>> {
        component_counter_metrics(interval, &|m| m.name() == "events_in_total")
            .map(|m| m.into_iter().map(ComponentEventsInTotal::new).collect())
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

    /// Total outgoing events metrics
    #[graphql(deprecation = "Use sent_events_total instead")]
    async fn events_out_total(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = EventsOutTotal> {
        get_metrics(interval).filter_map(|m| match m.name() {
            "events_out_total" => Some(EventsOutTotal::new(m)),
            _ => None,
        })
    }

    /// Total sent events metrics
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
    #[graphql(deprecation = "Use sent_events_throughput instead")]
    async fn events_out_throughput(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = i64> {
        counter_throughput(interval, &|m| m.name() == "events_out_total")
            .map(|(_, throughput)| throughput as i64)
    }

    /// Total outgoing events throughput sampled over the provided millisecond `interval`
    async fn sent_events_throughput(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = i64> {
        counter_throughput(interval, &|m| m.name() == "component_sent_events_total")
            .map(|(_, throughput)| throughput as i64)
    }

    /// Total outgoing component event throughput metrics over `interval`
    #[graphql(deprecation = "Use component_sent_events_throughputs instead")]
    async fn component_events_out_throughputs(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentEventsOutThroughput>> {
        component_counter_throughputs(interval, &|m| m.name() == "events_out_total").map(|m| {
            m.into_iter()
                .map(|(m, throughput)| {
                    ComponentEventsOutThroughput::new(
                        ComponentKey::from(m.tag_value("component_id").unwrap()),
                        throughput as i64,
                    )
                })
                .collect()
        })
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
    #[graphql(deprecation = "Use component_sent_events_totals instead")]
    async fn component_events_out_totals(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentEventsOutTotal>> {
        component_counter_metrics(interval, &|m| m.name() == "events_out_total")
            .map(|m| m.into_iter().map(ComponentEventsOutTotal::new).collect())
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

    /// Byte processing metrics.
    async fn processed_bytes_total(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = ProcessedBytesTotal> {
        get_metrics(interval).filter_map(|m| match m.name() {
            "processed_bytes_total" => Some(ProcessedBytesTotal::new(m)),
            _ => None,
        })
    }

    /// Byte processing throughput sampled over a provided millisecond `interval`.
    async fn processed_bytes_throughput(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = i64> {
        counter_throughput(interval, &|m| m.name() == "processed_bytes_total")
            .map(|(_, throughput)| throughput as i64)
    }

    /// Component byte processing metrics over `interval`.
    async fn component_processed_bytes_totals(
        &self,
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
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
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = Vec<ComponentProcessedBytesThroughput>> {
        component_counter_throughputs(interval, &|m| m.name() == "processed_bytes_total").map(|m| {
            m.into_iter()
                .map(|(m, throughput)| {
                    ComponentProcessedBytesThroughput::new(
                        ComponentKey::from(m.tag_value("component_id").unwrap()),
                        throughput as i64,
                    )
                })
                .collect()
        })
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
            "processed_events_total" => Some(MetricType::ProcessedEventsTotal(m.into())),
            "processed_bytes_total" => Some(MetricType::ProcessedBytesTotal(m.into())),
            _ => None,
        })
    }
}
