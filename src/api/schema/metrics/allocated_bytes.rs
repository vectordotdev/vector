use async_graphql::Object;
use chrono::{DateTime, Utc};

use crate::{
    config::ComponentKey,
    event::{Metric, MetricValue},
};

pub struct AllocatedBytes(Metric);

impl AllocatedBytes {
    pub const fn new(m: Metric) -> Self {
        Self(m)
    }
}

#[Object]
impl AllocatedBytes {
    /// Metric timestamp
    pub async fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.0.timestamp()
    }

    /// Allocated bytes
    pub async fn allocated_bytes(&self) -> f64 {
        match self.0.value() {
            MetricValue::Gauge { value } => *value,
            _ => 0.00,
        }
    }
}

impl From<Metric> for AllocatedBytes {
    fn from(m: Metric) -> Self {
        Self(m)
    }
}

pub struct ComponentAllocatedBytes {
    component_key: ComponentKey,
    metric: Metric,
}

impl ComponentAllocatedBytes {
    /// Returns a new `ComponentAllocatedBytes` struct, which is a GraphQL type. The
    /// component id is hoisted for clear field resolution in the resulting payload
    pub fn new(metric: Metric) -> Self {
        let component_key = metric.tag_value("component_id").expect(
            "Returned a metric without a `component_id`, which shouldn't happen. Please report.",
        );
        let component_key = ComponentKey::from(component_key);

        Self {
            component_key,
            metric,
        }
    }
}

#[Object]
impl ComponentAllocatedBytes {
    /// Component id
    async fn component_id(&self) -> &str {
        self.component_key.id()
    }

    /// Allocated bytes metric
    async fn metric(&self) -> AllocatedBytes {
        AllocatedBytes::new(self.metric.clone())
    }
}
