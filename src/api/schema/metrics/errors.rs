use crate::config::ComponentId;
use crate::event::{Metric, MetricValue};
use async_graphql::Object;
use chrono::{DateTime, Utc};

pub struct ErrorsTotal(Metric);

impl ErrorsTotal {
    pub fn new(m: Metric) -> Self {
        Self(m)
    }
}

#[Object]
impl ErrorsTotal {
    /// Metric timestamp
    pub async fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.0.timestamp()
    }

    /// Total error count
    pub async fn errors_total(&self) -> f64 {
        match self.0.value() {
            MetricValue::Counter { value } => *value,
            _ => 0.00,
        }
    }
}

impl From<Metric> for ErrorsTotal {
    fn from(m: Metric) -> Self {
        Self(m)
    }
}

pub struct ComponentErrorsTotal {
    component_id: ComponentId,
    metric: Metric,
}

impl ComponentErrorsTotal {
    /// Returns a new `ComponentErrorsTotal` struct, which is a GraphQL type. The
    /// component id is hoisted for clear field resolution in the resulting payload
    pub fn new(metric: Metric) -> Self {
        let component_id = metric.tag_value("component_id").expect(
            "Returned a metric without a `component_id`, which shouldn't happen. Please report.",
        );
        let component_id = ComponentId::from((metric.tag_value("pipeline_id"), component_id));

        Self {
            component_id,
            metric,
        }
    }
}

#[Object]
impl ComponentErrorsTotal {
    /// Component id
    async fn component_id(&self) -> &str {
        self.component_id.id()
    }

    /// Pipeline id
    async fn pipeline_id(&self) -> Option<&str> {
        self.component_id.pipeline_str()
    }

    /// Errors processed metric
    async fn metric(&self) -> ErrorsTotal {
        ErrorsTotal::new(self.metric.clone())
    }
}
