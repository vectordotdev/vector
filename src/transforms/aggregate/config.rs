use std::collections::HashMap;

use vector_lib::configurable::configurable_component;

use super::Aggregate;
use crate::{
    config::{DataType, Input, OutputId, TransformConfig, TransformContext, TransformOutput},
    schema,
    transforms::Transform,
};

/// Configuration for the `aggregate` transform.
#[configurable_component(transform("aggregate", "Aggregate metrics passing through a topology."))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AggregateConfig {
    /// The interval between flushes, in milliseconds.
    ///
    /// During this time frame, metrics (beta) with the same series data (name, namespace, tags, and so on) are aggregated.
    #[serde(default = "default_interval_ms")]
    #[configurable(metadata(docs::human_name = "Flush Interval"))]
    pub interval_ms: u64,
    /// Function to use for aggregation.
    ///
    /// Some of the functions may only function on incremental and some only on absolute metrics.
    #[serde(default = "default_mode")]
    pub mode: AggregationMode,
}

#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[configurable(description = "The aggregation mode to use.")]
pub enum AggregationMode {
    /// Default mode. Sums incremental metrics and uses the latest value for absolute metrics.
    #[default]
    Auto,

    /// Sums incremental metrics; absolute metrics pass through unchanged.
    Sum,

    /// Returns the latest value for absolute metrics; incremental metrics pass through unchanged.
    Latest,

    /// Counts metrics for incremental and absolute metrics
    Count,

    /// Returns difference between latest value for absolute; incremental metrics pass through unchanged.
    Diff,

    /// Max value of absolute metric; incremental metrics pass through unchanged.
    Max,

    /// Min value of absolute metric; incremental metrics pass through unchanged.
    Min,

    /// Mean value of absolute metric; incremental metrics pass through unchanged.
    Mean,

    /// Stdev value of absolute metric; incremental metrics pass through unchanged.
    Stdev,
}
const fn default_mode() -> AggregationMode {
    AggregationMode::Auto
}

const fn default_interval_ms() -> u64 {
    10 * 1000
}

impl_generate_config_from_default!(AggregateConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "aggregate")]
impl TransformConfig for AggregateConfig {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        Aggregate::new(self).map(Transform::event_task)
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn outputs(
        &self,
        _: &TransformContext,
        _: &[(OutputId, schema::Definition)],
    ) -> Vec<TransformOutput> {
        vec![TransformOutput::new(DataType::Metric, HashMap::new())]
    }
}
