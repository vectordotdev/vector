use std::collections::HashMap;

use vector_lib::configurable::configurable_component;

use crate::{
    config::{
        DataType, GenerateConfig, Input, OutputId, TransformConfig, TransformContext,
        TransformOutput,
    },
    schema,
    transforms::{Transform, tag_cardinality_limit::TagCardinalityLimit},
};

// Top-level configuration

/// Configuration for the `tag_cardinality_limit` transform.
#[configurable_component(transform(
    "tag_cardinality_limit",
    "Limit the cardinality of tags on metrics events as a safeguard against cardinality explosion."
))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    #[serde(flatten)]
    pub global: Inner,

    /// Controls how tag tracking state is partitioned across metrics.
    #[configurable(derived)]
    #[serde(default)]
    pub tracking_scope: TrackingScope,

    /// Maximum number of distinct (metric, tag-key) pairs to track across the entire
    /// transform. When this cap is reached, additional tag keys on new metrics or new
    /// tag keys on existing metrics are not tracked, and tag values for those pairs
    /// pass through unchecked. Users can detect this via the
    /// `tag_cardinality_untracked_events_total` counter and the
    /// `tag_cardinality_tracked_keys` gauge.
    ///
    /// When unset (default), there is no cap and the transform tracks all pairs it
    /// encounters. In `global` tracking scope mode, this limit still applies (the
    /// metric key is set to `None` unless there is a per-metric override).
    #[configurable(derived)]
    #[serde(default)]
    pub max_tracked_keys: Option<usize>,

    /// Tag cardinality limits configuration per metric name.
    #[configurable(
        derived,
        metadata(docs::additional_props_description = "An individual metric configuration.")
    )]
    #[serde(default)]
    pub per_metric_limits: HashMap<String, PerMetricConfig>,

    /// Global per-tag-key overrides. Each entry sets a `mode`:
    /// - `mode: limit_override` + `value_limit: N` — track with a per-tag cap.
    /// - `mode: excluded` — opt this tag out of tracking entirely (passed through unchanged
    ///   for every metric, never counted against `value_limit`, and never added to the cache).
    ///
    /// Useful for tag keys whose high cardinality is intentional on every metric (for example,
    /// `kube_pod_name` or `tenant_id`), or for narrowing the cap on a single tag without
    /// redefining the entire global limit.
    ///
    /// Per-metric overrides take precedence: when a metric has a matching `per_metric_limits`
    /// entry, only that entry's `per_tag_limits` is consulted for that metric; this top-level
    /// `per_tag_limits` is ignored. Tags not listed at either level fall back to the
    /// applicable metric-level configuration.
    #[configurable(
        derived,
        metadata(docs::additional_props_description = "An individual tag configuration.")
    )]
    #[serde(default)]
    pub per_tag_limits: HashMap<String, PerTagConfig>,
}

/// Controls how tag tracking state is partitioned across metrics.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TrackingScope {
    /// All metrics share a single tracking bucket. Tag values pool across metrics
    /// and the global `value_limit` caps the combined set.
    #[default]
    Global,

    /// Every distinct metric gets its own tracking bucket, providing tag
    /// cardinality limiting for each metric in isolation at the cost of higher
    /// memory usage.
    PerMetric,
}

/// Configuration block used at the global level.
#[configurable_component]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Inner {
    /// How many distinct values to accept for any given key.
    #[serde(default = "default_value_limit")]
    pub value_limit: usize,

    #[configurable(derived)]
    #[serde(default = "default_limit_exceeded_action")]
    pub limit_exceeded_action: LimitExceededAction,

    #[serde(flatten)]
    pub mode: Mode,

    #[configurable(derived)]
    #[serde(default)]
    pub internal_metrics: InternalMetricsConfig,
}

/// Controls the approach taken for tracking tag cardinality at the global level.
#[configurable_component]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
#[configurable(metadata(
    docs::enum_tag_description = "Controls the approach taken for tracking tag cardinality."
))]
pub enum Mode {
    /// Tracks cardinality exactly.
    ///
    /// This mode has higher memory requirements than `probabilistic`, but never falsely outputs
    /// metrics with new tags after the limit has been hit.
    Exact,

    /// Tracks cardinality probabilistically.
    ///
    /// This mode has lower memory requirements than `exact`, but may occasionally allow metric
    /// events to pass through the transform even when they contain new tags that exceed the
    /// configured limit. The rate at which this happens can be controlled by changing the value of
    /// `cache_size_per_key`.
    Probabilistic(BloomFilterConfig),
}

/// Per-metric name tag cardinality limit configuration.
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PerMetricConfig {
    /// Namespace of the metric this configuration refers to.
    #[serde(default)]
    pub namespace: Option<String>,

    /// Per-tag-key overrides scoped to this metric. Each entry sets a `mode`:
    /// - `mode: limit_override` + `value_limit: N` — track with a per-tag cap.
    /// - `mode: excluded` — opt this tag out of tracking entirely.
    ///
    ///  All other settings (tracking algorithm, `limit_exceeded_action`, etc.)
    /// are inherited from the enclosing per-metric configuration.
    /// Tags not listed here use the per-metric configuration.
    #[configurable(
        derived,
        metadata(docs::additional_props_description = "An individual tag configuration.")
    )]
    #[serde(default)]
    pub per_tag_limits: HashMap<String, PerTagConfig>,

    #[serde(flatten)]
    pub config: OverrideInner,
}

/// Configuration block used at per-metric level. Same shape as the global configuration but
/// with `OverrideMode`, which adds `excluded` for opting that metric out of cardinality
/// control entirely.
#[configurable_component]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverrideInner {
    /// How many distinct values to accept for any given key. Ignored when `mode: excluded`.
    #[serde(default = "default_value_limit")]
    pub value_limit: usize,

    #[configurable(derived)]
    #[serde(default = "default_limit_exceeded_action")]
    pub limit_exceeded_action: LimitExceededAction,

    #[serde(flatten)]
    pub mode: OverrideMode,

    #[configurable(derived)]
    #[serde(default)]
    pub internal_metrics: InternalMetricsConfig,
}

/// Controls the approach taken for tracking tag cardinality at the per-metric level.
/// Adds `excluded` to the global `Mode` variants to allow opting a metric out entirely.
#[configurable_component]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
#[configurable(metadata(
    docs::enum_tag_description = "Controls the approach taken for tracking tag cardinality."
))]
pub enum OverrideMode {
    /// Tracks cardinality exactly. See `Mode::Exact` for details.
    Exact,

    /// Tracks cardinality probabilistically. See `Mode::Probabilistic` for details.
    Probabilistic(BloomFilterConfig),

    /// Skip cardinality tracking for this metric. All tag values pass through and nothing is
    /// limited. Other fields in this per-metric configuration are ignored when this is selected.
    Excluded,
}

impl OverrideMode {
    /// Returns the equivalent global `Mode` if this scope is tracked, or `None` if excluded.
    pub const fn as_mode(&self) -> Option<Mode> {
        match self {
            OverrideMode::Exact => Some(Mode::Exact),
            OverrideMode::Probabilistic(b) => Some(Mode::Probabilistic(*b)),
            OverrideMode::Excluded => None,
        }
    }
}

/// Per-tag cardinality configuration.
///
/// Specify `mode` to control how this tag is handled:
///
/// Example:
/// ```yaml
/// per_tag_limits:
///   environment:
///     mode: limit_override  # track with a per-tag cap
///     value_limit: 3
///   trace_id:
///     mode: excluded        # opt out of tracking entirely
/// ```
#[configurable_component]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PerTagConfig {
    #[configurable(derived)]
    #[serde(flatten)]
    pub mode: PerTagMode,
}

/// Mode applied to a specific tag key within a per-metric override.
///
/// The tracking algorithm (`exact`/`probabilistic`), `cache_size_per_key`,
/// `limit_exceeded_action`, and `internal_metrics` are always inherited from the
/// enclosing per-metric configuration.
#[configurable_component]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
#[configurable(metadata(docs::enum_tag_description = "Controls how this tag key is handled."))]
pub enum PerTagMode {
    /// Track this tag with a per-tag value limit. The enclosing per-metric tracking
    /// algorithm and all other settings still apply.
    LimitOverride {
        /// Maximum number of distinct values to accept for this tag key.
        value_limit: usize,
    },
    /// Opt this tag out of cardinality tracking entirely. All values pass through
    /// without being recorded or checked against any `value_limit`.
    Excluded,
}

// =============================================================================
// Shared building blocks
// =============================================================================

/// Possible actions to take when an event arrives that would exceed the cardinality limit for one
/// or more of its tags.
#[configurable_component]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LimitExceededAction {
    /// Drop the tag(s) that would exceed the configured limit.
    DropTag,

    /// Drop the entire event itself.
    DropEvent,
}

/// Bloom filter configuration in probabilistic mode.
#[configurable_component]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BloomFilterConfig {
    /// The size of the cache for detecting duplicate tags, in bytes.
    ///
    /// The larger the cache size, the less likely it is to have a false positive, or a case where
    /// we allow a new value for tag even after we have reached the configured limits.
    #[serde(default = "default_cache_size")]
    #[configurable(metadata(docs::human_name = "Cache Size per Key"))]
    pub cache_size_per_key: usize,
}

/// Configuration of internal metrics for the TagCardinalityLimit transform.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct InternalMetricsConfig {
    /// Whether to include extended tags (metric_name, tag_key) in the `tag_value_limit_exceeded_total` metric.
    ///
    /// This helps identify which metrics and tag keys are hitting cardinality limits, but can significantly
    /// increase metric cardinality. Defaults to `false` because these tags have potentially unbounded cardinality.
    #[serde(default = "default_include_extended_tags")]
    #[configurable(metadata(docs::human_name = "Include Extended Tags"))]
    pub include_extended_tags: bool,
}

// =============================================================================
// Defaults
// =============================================================================

const fn default_value_limit() -> usize {
    500
}

const fn default_limit_exceeded_action() -> LimitExceededAction {
    LimitExceededAction::DropTag
}

const fn default_include_extended_tags() -> bool {
    false
}

pub(crate) const fn default_cache_size() -> usize {
    5 * 1024 // 5KB
}

// =============================================================================
// Transform plumbing
// =============================================================================

impl GenerateConfig for Config {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            global: Inner {
                mode: Mode::Exact,
                value_limit: default_value_limit(),
                limit_exceeded_action: default_limit_exceeded_action(),
                internal_metrics: InternalMetricsConfig::default(),
            },
            tracking_scope: TrackingScope::default(),
            max_tracked_keys: None,
            per_metric_limits: HashMap::default(),
            per_tag_limits: HashMap::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "tag_cardinality_limit")]
impl TransformConfig for Config {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::event_task(TagCardinalityLimit::new(
            self.clone(),
        )))
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
