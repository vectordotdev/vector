mod generic;

use super::{ProcessedBytesTotal, ProcessedEventsTotal};
use crate::event::Metric;
use async_graphql::Interface;

#[derive(Debug, Clone, Interface)]
#[graphql(
    field(name = "processed_events_total", type = "Option<ProcessedEventsTotal>"),
    field(name = "processed_bytes_total", type = "Option<ProcessedBytesTotal>")
)]
pub enum SinkMetrics {
    GenericSinkMetrics(generic::GenericSinkMetrics),
}

pub trait IntoSinkMetrics {
    fn into_sink_metrics(self, component_type: &str) -> SinkMetrics;
}

impl IntoSinkMetrics for Vec<Metric> {
    fn into_sink_metrics(self, _component_type: &str) -> SinkMetrics {
        SinkMetrics::GenericSinkMetrics(generic::GenericSinkMetrics::new(self))
    }
}
