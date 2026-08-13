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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AggregateConfig {
    /// The interval between flushes, in milliseconds.
    ///
    /// Must be greater than zero. During this time frame, metrics (beta) with the same series data
    /// (name, namespace, tags, and so on) are aggregated.
    #[serde(default = "default_interval_ms")]
    #[configurable(metadata(docs::human_name = "Flush Interval"))]
    pub interval_ms: u64,
    /// Function to use for aggregation.
    ///
    /// Some of the functions may only function on incremental and some only on absolute metrics.
    #[serde(default = "default_mode")]
    #[configurable(derived)]
    pub mode: AggregationMode,
    /// Time source to use for aggregation windows.
    ///
    /// When set to `event_time`, events are grouped into buckets based on their timestamps rather than
    /// when they are processed. Events are rejected when their window has ended (per
    /// `allowed_lateness_ms`) or when their bucket was already emitted (watermark).
    #[serde(default = "default_time_source")]
    #[configurable(derived)]
    pub time_source: TimeSource,

    /// Grace period for late-arriving events when using event-time aggregation.
    ///
    /// Each bucket accepts events until the system clock reaches
    /// `bucket_end + allowed_lateness_ms`, where `bucket_end` is the exclusive end of the
    /// event-time window. That cutoff is enforced when events are recorded, not only when a
    /// periodic flush runs. Once a bucket is emitted it is closed permanently; any later
    /// events whose timestamp falls inside it are dropped and counted via
    /// `component_discarded_events_total`.
    ///
    /// Set to 0 for strict ordering (no late events allowed). Only applies when `time_source`
    /// is set to `event_time`.
    #[serde(default)]
    #[configurable(metadata(docs::examples = 0, docs::examples = 5000, docs::examples = 30000))]
    pub allowed_lateness_ms: u64,

    /// How to handle events with missing timestamps in event-time mode.
    ///
    /// When `true`, events without a timestamp use the current system time as a fallback.
    /// When `false`, such events are dropped and counted via `component_discarded_events_total`.
    ///
    /// Only applies when `time_source` is set to `event_time`.
    #[serde(default)]
    pub use_system_time_for_missing_timestamps: bool,

    /// Maximum allowed time drift for future events in event-time mode.
    ///
    /// Acts as a clock-skew guard: events whose timestamp is further in the future than this
    /// many milliseconds (relative to the current system time) are dropped and counted via
    /// `component_discarded_events_total`. Defaults to 10 seconds.
    ///
    /// Set to 0 to allow events at any future time. Only applies when `time_source` is set
    /// to `event_time`.
    #[serde(default = "default_max_future_ms")]
    #[configurable(metadata(docs::examples = 0, docs::examples = 60000, docs::examples = 300000))]
    pub max_future_ms: u64,
}

#[configurable_component]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[configurable(description = "The time source to use for aggregation windows.")]
#[serde(rename_all = "snake_case")]
pub enum TimeSource {
    /// Use system clock time for aggregation windows (default).
    ///
    /// Events are aggregated based on when they are processed, not their timestamps.
    #[default]
    SystemTime,

    /// Use event timestamps for aggregation windows.
    ///
    /// Events are grouped into buckets based on their timestamps. Events are rejected when
    /// their window has ended or their bucket was already emitted.
    EventTime,
}

const fn default_time_source() -> TimeSource {
    TimeSource::SystemTime
}

pub(crate) const fn default_max_future_ms() -> u64 {
    10000 // 10 seconds default
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

impl Default for AggregateConfig {
    fn default() -> Self {
        Self {
            interval_ms: default_interval_ms(),
            mode: default_mode(),
            time_source: default_time_source(),
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: default_max_future_ms(),
        }
    }
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
