use async_graphql::Object;
use chrono::{DateTime, Utc};
use vector_core::event::metric::MetricTags;

use super::EventEncodingType;
use crate::{
    config::OutputId,
    event::{self},
};

#[derive(Debug, Clone)]
pub struct Metric {
    output_id: OutputId,
    event: event::Metric,
}

impl Metric {
    pub const fn new(output_id: OutputId, event: event::Metric) -> Self {
        Self { output_id, event }
    }
}

#[Object]
/// Metric event with fields for querying metric data
impl Metric {
    /// Id of the component associated with the metric event
    async fn component_id(&self) -> &str {
        self.output_id.component.id()
    }

    /// Metric timestamp
    async fn timestamp(&self) -> Option<&DateTime<Utc>> {
        self.event.data().timestamp().as_ref()
    }

    /// Metric name
    async fn name(&self) -> &str {
        self.event.name()
    }

    /// Metric namespace
    async fn namespace(&self) -> Option<&str> {
        self.event.namespace()
    }

    /// Metric kind
    async fn kind(&self) -> event::MetricKind {
        self.event.kind()
    }

    /// Metric type
    async fn value_type(&self) -> &str {
        self.event.value().as_name()
    }

    /// Metric value in human readable form
    async fn value(&self) -> String {
        self.event.value().to_string()
    }

    /// Metric tags
    async fn tags(&self) -> Option<&MetricTags> {
        self.event.tags()
    }

    /// Metric event as an encoded string format
    async fn string(&self, encoding: EventEncodingType) -> String {
        match encoding {
            EventEncodingType::Json => serde_json::to_string(&self.event)
                .expect("JSON serialization of metric event failed. Please report."),
            EventEncodingType::Yaml => serde_yaml::to_string(&self.event)
                .expect("YAML serialization of metric event failed. Please report."),
        }
    }
}
