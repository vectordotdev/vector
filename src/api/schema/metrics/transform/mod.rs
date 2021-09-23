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
pub enum TransformMetrics {
    GenericTransformMetrics(generic::GenericTransformMetrics),
}

pub trait IntoTransformMetrics {
    fn into_transform_metrics(self, component_type: &str) -> TransformMetrics;
}

impl IntoTransformMetrics for Vec<Metric> {
    fn into_transform_metrics(self, _component_type: &str) -> TransformMetrics {
        TransformMetrics::GenericTransformMetrics(generic::GenericTransformMetrics::new(self))
    }
}
