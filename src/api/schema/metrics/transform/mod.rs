mod generic;

use super::{ProcessedBytesTotal, ProcessedEventsTotal};
use crate::event::Metric;
use async_graphql::Interface;

#[derive(Debug, Clone, Interface)]
#[graphql(
    field(name = "processed_events_total", type = "Option<ProcessedEventsTotal>"),
    field(name = "processed_bytes_total", type = "Option<ProcessedBytesTotal>")
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
