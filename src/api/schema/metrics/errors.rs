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
        self.0.data.timestamp
    }

    /// Total error count
    pub async fn errors_total(&self) -> f64 {
        match self.0.data.value {
            MetricValue::Counter { value } => value,
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
    name: String,
    metric: Metric,
}

impl ComponentErrorsTotal {
    /// Returns a new `ComponentErrorsTotal` struct, which is a GraphQL type. The
    /// component name is hoisted for clear field resolution in the resulting payload
    pub fn new(metric: Metric) -> Self {
        let name = metric.tag_value("component_name").expect(
            "Returned a metric without a `component_name`, which shouldn't happen. Please report.",
        );

        Self { name, metric }
    }
}

#[Object]
impl ComponentErrorsTotal {
    /// Component name
    async fn name(&self) -> &str {
        &self.name
    }

    /// Errors processed metric
    async fn metric(&self) -> ErrorsTotal {
        ErrorsTotal::new(self.metric.clone())
    }
}
