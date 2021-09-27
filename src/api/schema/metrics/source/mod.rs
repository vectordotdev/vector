pub mod file;
mod generic;

use super::{
    EventsInTotal, EventsOutTotal, ProcessedBytesTotal, ProcessedEventsTotal, ReceivedEventsTotal,
};
use crate::event::Metric;
use async_graphql::Interface;

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
    field(name = "events_out_total", type = "Option<EventsOutTotal>")
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
