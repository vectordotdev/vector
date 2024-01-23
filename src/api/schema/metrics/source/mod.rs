pub mod file;
mod generic;

use async_graphql::Interface;

use super::{ReceivedBytesTotal, ReceivedEventsTotal, SentEventsTotal};
use crate::event::Metric;

#[derive(Debug, Clone, Interface)]
#[graphql(
    field(name = "received_bytes_total", ty = "Option<ReceivedBytesTotal>"),
    field(name = "received_events_total", ty = "Option<ReceivedEventsTotal>"),
    field(name = "sent_events_total", ty = "Option<SentEventsTotal>")
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
