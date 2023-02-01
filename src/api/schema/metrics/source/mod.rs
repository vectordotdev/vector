pub mod file;
mod generic;

use async_graphql::Interface;

use super::{
    EventsInTotal, EventsOutTotal, ProcessedBytesTotal, ProcessedEventsTotal, ReceivedEventsTotal,
    SentEventsTotal,
};
use crate::event::Metric;

#[derive(Debug, Clone, Interface)]
#[graphql(
    field(name = "processed_events_total", type = "Option<ProcessedEventsTotal>"),
    field(name = "processed_bytes_total", type = "Option<ProcessedBytesTotal>"),
    field(name = "received_events_total", type = "Option<ReceivedEventsTotal>"),
    field(
        name = "events_in_total",
        type = "Option<EventsInTotal>",
        deprecation = "Use received_events_total instead"
    ),
    field(name = "sent_events_total", type = "Option<SentEventsTotal>"),
    field(
        name = "events_out_total",
        type = "Option<EventsOutTotal>",
        deprecation = "Use sent_events_total instead"
    )
)]
pub enum SourceMetrics {
    GenericSourceMetrics(generic::GenericSourceMetrics),
    FileSourceMetrics(file::FileSourceMetrics),
}

pub trait IntoSourceMetrics {
    fn into_source_metrics(self, component_type: &str) -> SourceMetrics;
}

impl IntoSourceMetrics for Vec<Metric> {
    fn into_source_metrics(self, component_type: &str) -> SourceMetrics {
        match component_type {
            "file" => SourceMetrics::FileSourceMetrics(file::FileSourceMetrics::new(self)),
            _ => SourceMetrics::GenericSourceMetrics(generic::GenericSourceMetrics::new(self)),
        }
    }
}
