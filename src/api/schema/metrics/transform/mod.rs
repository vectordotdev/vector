mod generic;

use async_graphql::Interface;

use super::{ReceivedEventsTotal, SentEventsTotal};
use crate::event::Metric;

#[derive(Debug, Clone, Interface)]
#[graphql(
    field(name = "received_events_total", ty = "Option<ReceivedEventsTotal>"),
    field(name = "sent_events_total", ty = "Option<SentEventsTotal>")
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
