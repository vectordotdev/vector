use std::time::Duration;

use governor::clock;
use serde_with::serde_as;
use vector_lib::{config::clone_input_definitions, configurable::configurable_component};

use super::{DROPPED, transform::Throttle};
use crate::{
    conditions::AnyCondition,
    config::{DataType, Input, OutputId, TransformConfig, TransformContext, TransformOutput},
    schema,
    template::Template,
    transforms::Transform,
};

/// Configuration of internal metrics for the Throttle transform.
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct ThrottleInternalMetricsConfig {
    /// Whether or not to emit the `events_discarded_total` internal metric with the `key` tag.
    ///
    /// If true, the counter will be incremented for each discarded event, including the key value
    /// associated with the discarded event. If false, the counter will not be emitted. Instead, the
    /// number of discarded events can be seen through the `component_discarded_events_total` internal
    /// metric.
    ///
    /// Note that this defaults to false because the `key` tag has potentially unbounded cardinality.
    /// Only set this to true if you know that the number of unique keys is bounded.
    #[serde(default)]
    pub emit_events_discarded_per_key: bool,

    /// Whether to emit detailed per-key per-threshold-type metrics including discard counts,
    /// bytes/tokens processed, and utilization ratio gauges.
    ///
    /// WARNING: Cardinality scales with the number of unique `key_field` values. Only enable
    /// when you know the key cardinality is bounded.
    #[serde(default)]
    pub emit_detailed_metrics: bool,
}

/// Multi-dimensional threshold configuration.
///
/// Each field defines an independent rate limit. An event is dropped when *any* threshold
/// is exceeded for its key within the configured window.
#[configurable_component]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MultiThresholdConfig {
    /// Maximum number of events allowed per key per window.
    #[serde(default)]
    pub events: Option<u32>,

    /// Maximum estimated JSON-encoded bytes allowed per key per window.
    ///
    /// Uses Vector's `EstimatedJsonEncodedSizeOf` trait for fast size estimation
    /// without actual serialization.
    #[serde(default)]
    pub json_bytes: Option<u32>,

    /// A VRL expression evaluated per event that returns a numeric cost.
    ///
    /// The result is used as the number of tokens to consume from the rate limiter bucket.
    /// For example, `strlen(string!(.message))` throttles by message length.
    #[configurable(metadata(
        docs::examples = "strlen(string!(.message))",
        docs::examples = "to_int(.cost) ?? 1",
    ))]
    #[serde(default)]
    pub tokens: Option<String>,
}

/// Threshold configuration supporting both simple (backward-compatible) and multi-dimensional forms.
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum ThresholdConfig {
    /// Simple event-count threshold (backward compatible): `threshold: 100`
    Simple(u32),

    /// Multi-dimensional threshold with independent limits for events, bytes, and/or tokens.
    Multi(MultiThresholdConfig),
}

impl Default for ThresholdConfig {
    fn default() -> Self {
        // Default to 0 for generate_config; real configs must set a value.
        ThresholdConfig::Simple(0)
    }
}

impl ThresholdConfig {
    /// Returns the effective events threshold, if configured.
    pub const fn events_threshold(&self) -> Option<u32> {
        match self {
            ThresholdConfig::Simple(n) => Some(*n),
            ThresholdConfig::Multi(m) => m.events,
        }
    }

    /// Returns the effective json_bytes threshold, if configured.
    pub const fn json_bytes_threshold(&self) -> Option<u32> {
        match self {
            ThresholdConfig::Simple(_) => None,
            ThresholdConfig::Multi(m) => m.json_bytes,
        }
    }

    /// Returns the VRL tokens expression, if configured.
    pub fn tokens_expression(&self) -> Option<&str> {
        match self {
            ThresholdConfig::Simple(_) => None,
            ThresholdConfig::Multi(m) => m.tokens.as_deref(),
        }
    }

    /// Returns true if at least one threshold is configured (non-zero).
    pub fn has_any_threshold(&self) -> bool {
        match self {
            ThresholdConfig::Simple(n) => *n > 0,
            ThresholdConfig::Multi(m) => {
                m.events.is_some_and(|n| n > 0)
                    || m.json_bytes.is_some_and(|n| n > 0)
                    || m.tokens.is_some()
            }
        }
    }
}

/// Configuration for the `throttle` transform.
#[serde_as]
#[configurable_component(transform(
    "throttle",
    "Rate limit logs passing through a topology by events, bytes, or custom VRL token cost."
))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ThrottleConfig {
    /// The rate limiting threshold(s).
    ///
    /// Accepts either a simple integer (number of events) for backward compatibility,
    /// or an object with `events`, `json_bytes`, and/or `tokens` fields for
    /// multi-dimensional rate limiting.
    pub threshold: ThresholdConfig,

    /// The time window in which the configured `threshold` is applied, in seconds.
    #[serde_as(as = "serde_with::DurationSecondsWithFrac<f64>")]
    #[configurable(metadata(docs::human_name = "Time Window"))]
    pub window_secs: Duration,

    /// The value to group events into separate buckets to be rate limited independently.
    ///
    /// If left unspecified, or if the event doesn't have `key_field`, then the event is not rate
    /// limited separately.
    #[configurable(metadata(docs::examples = "{{ message }}", docs::examples = "{{ hostname }}",))]
    pub key_field: Option<Template>,

    /// A logical condition used to exclude events from sampling.
    pub exclude: Option<AnyCondition>,

    /// Whether to route dropped events to a named `dropped` output instead of discarding them.
    ///
    /// When enabled, events that exceed the rate limit are sent to the `.dropped` output port,
    /// which can be connected to a dead-letter sink.
    #[serde(default)]
    pub reroute_dropped: bool,

    #[configurable(derived)]
    #[serde(default)]
    pub internal_metrics: ThrottleInternalMetricsConfig,
}

impl_generate_config_from_default!(ThrottleConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "throttle")]
impl TransformConfig for ThrottleConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        Throttle::new(self, context, clock::MonotonicClock)
            .map(|t| Transform::synchronous(t.into_sync_transform()))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn outputs(
        &self,
        _: &TransformContext,
        input_definitions: &[(OutputId, schema::Definition)],
    ) -> Vec<TransformOutput> {
        // The event is not modified, so the definition is passed through as-is
        let default_output = TransformOutput::new(
            DataType::Log,
            clone_input_definitions(input_definitions),
        );

        if self.reroute_dropped {
            vec![
                default_output,
                TransformOutput::new(
                    DataType::Log,
                    clone_input_definitions(input_definitions),
                )
                .with_port(DROPPED),
            ]
        } else {
            vec![default_output]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ThrottleConfig;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<ThrottleConfig>();
    }

    #[test]
    fn parse_simple_threshold() {
        let config: ThrottleConfig = toml::from_str(
            r"
threshold = 100
window_secs = 10
",
        )
        .unwrap();
        assert_eq!(config.threshold.events_threshold(), Some(100));
        assert_eq!(config.threshold.json_bytes_threshold(), None);
        assert_eq!(config.threshold.tokens_expression(), None);
    }

    #[test]
    fn parse_multi_threshold_events_only() {
        let config: ThrottleConfig = toml::from_str(
            r"
window_secs = 10

[threshold]
events = 500
",
        )
        .unwrap();
        assert_eq!(config.threshold.events_threshold(), Some(500));
        assert_eq!(config.threshold.json_bytes_threshold(), None);
    }

    #[test]
    fn parse_multi_threshold_all() {
        let config: ThrottleConfig = toml::from_str(
            r#"
window_secs = 60

[threshold]
events = 1000
json_bytes = 500000
tokens = "strlen(string!(.message))"
"#,
        )
        .unwrap();
        assert_eq!(config.threshold.events_threshold(), Some(1000));
        assert_eq!(config.threshold.json_bytes_threshold(), Some(500000));
        assert_eq!(config.threshold.tokens_expression(), Some("strlen(string!(.message))"));
    }

    #[test]
    fn parse_reroute_dropped() {
        let config: ThrottleConfig = toml::from_str(
            r"
threshold = 100
window_secs = 10
reroute_dropped = true
",
        )
        .unwrap();
        assert!(config.reroute_dropped);
    }

    #[test]
    fn parse_detailed_metrics() {
        let config: ThrottleConfig = toml::from_str(
            r"
threshold = 100
window_secs = 10

[internal_metrics]
emit_detailed_metrics = true
",
        )
        .unwrap();
        assert!(config.internal_metrics.emit_detailed_metrics);
    }
}
