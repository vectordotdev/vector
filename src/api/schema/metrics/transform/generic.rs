use async_graphql::Object;

use crate::{
    api::schema::metrics::{self, MetricsFilter},
    event::Metric,
};

#[derive(Debug, Clone)]
pub struct GenericTransformMetrics(Vec<Metric>);

impl GenericTransformMetrics {
    pub const fn new(metrics: Vec<Metric>) -> Self {
        Self(metrics)
    }
}

#[Object]
impl GenericTransformMetrics {
    /// Total received events for the current transform
    pub async fn received_events_total(&self) -> Option<metrics::ReceivedEventsTotal> {
        self.0.received_events_total()
    }

    /// Total sent events for the current transform
    pub async fn sent_events_total(&self) -> Option<metrics::SentEventsTotal> {
        self.0.sent_events_total()
    }
}
