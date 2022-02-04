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

    pub fn get_timestamp(&self) -> Option<&DateTime<Utc>> {
        self.event.data().timestamp().as_ref()
    }

    pub fn get_tags(&self) -> Option<&MetricTags> {
        self.event.series().tags.as_ref()
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
        self.get_timestamp()
    }

    /// Metric tags
    async fn tags(&self) -> Option<&MetricTags> {
        self.get_tags()
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
