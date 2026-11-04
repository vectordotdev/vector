use std::{collections::HashMap, time::Duration};

use governor::clock;
use serde_with::serde_as;
use vector_lib::{config::clone_input_definitions, configurable::configurable_component};

use super::transform::{ConfigError, Throttle};
use crate::{
    conditions::AnyCondition,
    config::{DataType, Input, OutputId, TransformConfig, TransformContext, TransformOutput},
    schema,
    template::UnconfinedTemplate,
    transforms::Transform,
};

/// Configuration of internal metrics for the Throttle transform.
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct ThrottleInternalMetricsConfig {
    /// Whether or not to emit the `events_discarded_total` internal metric with the `key` tag.
    ///
    /// This legacy option applies only when `events_discarded_tags` is not configured.
    /// When false, discard counts remain available through `component_discarded_events_total`.
    /// Defaults to false because the `key` tag has potentially unbounded cardinality.
    #[serde(default)]
    pub emit_events_discarded_per_key: bool,

    /// Tags to include on the `events_discarded_total` internal metric.
    ///
    /// Configuring this map enables the counter and replaces the legacy `key` tag selection,
    /// regardless of `emit_events_discarded_per_key`. An empty map emits the counter without
    /// attribution tags. Vector's component and collector tags are still added normally.
    /// When this option is absent, `emit_events_discarded_per_key` retains its existing behavior.
    /// To include a `key` tag, select it explicitly with a template, such as `key: "{{ workload_id }}"`.
    /// It is an ordinary log template, not a reference to Vector's resolved throttle key.
    ///
    /// Values are event templates evaluated only for discarded events. Missing fields emit a
    /// warning and omit that tag; empty results also omit it. These tags do not change rate-limit
    /// buckets or `component_discarded_events_total`. Vector's global metric tags, such as
    /// `component_id`, are reserved. Only use values with bounded cardinality.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[configurable(metadata(
        docs::additional_props_description = "An event template for a discard metric tag."
    ))]
    pub events_discarded_tags: Option<HashMap<String, UnconfinedTemplate>>,

    /// Whether or not to include the `group` tag on the `component_discarded_events_total`
    /// internal metric.
    ///
    /// When enabled, adds a `group` tag containing the rendered `key_field` value.
    /// If the `key_field` value is missing or can't be rendered, the tag value is `None`.
    ///
    /// Note that this defaults to false because the `group` tag has potentially unbounded
    /// cardinality. Only set this to true if you know that the number of unique groups is bounded.
    #[serde(default)]
    pub include_group_tag: bool,
}

impl ThrottleInternalMetricsConfig {
    pub(super) fn validate(&self) -> Result<(), ConfigError> {
        if let Some(tag) = self
            .events_discarded_tags
            .iter()
            .flat_map(|tags| tags.keys())
            .find(|tag| vector_lib::metrics::LABELS.contains(tag.as_str()))
        {
            return Err(ConfigError::ReservedMetricTag { tag: tag.clone() });
        }
        Ok(())
    }
}

/// Configuration for the `throttle` transform.
#[serde_as]
#[configurable_component(transform("throttle", "Rate limit logs passing through a topology."))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ThrottleConfig {
    /// The number of events allowed for a given bucket per configured `window_secs`.
    ///
    /// Each unique key has its own `threshold`.
    pub threshold: u32,

    /// The time window in which the configured `threshold` is applied, in seconds.
    #[serde_as(as = "serde_with::DurationSecondsWithFrac<f64>")]
    #[configurable(metadata(docs::human_name = "Time Window"))]
    pub window_secs: Duration,

    /// The value to group events into separate buckets to be rate limited independently.
    ///
    /// If left unspecified, or if the event doesn't have `key_field`, then the event is not rate
    /// limited separately.
    #[configurable(metadata(docs::examples = "{{ message }}", docs::examples = "{{ hostname }}",))]
    pub key_field: Option<UnconfinedTemplate>,

    /// A logical condition used to exclude events from sampling.
    pub exclude: Option<AnyCondition>,

    #[serde(default)]
    pub internal_metrics: ThrottleInternalMetricsConfig,
}

impl_generate_config_from_default!(ThrottleConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "throttle")]
impl TransformConfig for ThrottleConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        Throttle::new(self, context, clock::MonotonicClock).map(Transform::event_task)
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
        vec![TransformOutput::new(
            DataType::Log,
            clone_input_definitions(input_definitions),
        )]
    }

    fn validate_structure(&self) -> Result<(), Vec<String>> {
        if self.threshold == 0 || self.window_secs.as_secs_f64() == 0.0 {
            Err(vec![
                "`threshold` and `window_secs` must be non-zero".to_string(),
            ])
        } else {
            self.internal_metrics
                .validate()
                .map_err(|error| vec![error.to_string()])
        }
    }

    fn validate_with_context(&self, context: &TransformContext) -> Result<(), Vec<String>> {
        if let Some(Err(e)) = self
            .exclude
            .as_ref()
            .map(|c| c.validate(&context.enrichment_tables, &context.metrics_storage))
        {
            Err(vec![format!("exclude: {e}")])
        } else {
            Ok(())
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
    fn internal_metrics_defaults_preserve_existing_labels() {
        let config = serde_yaml::from_str::<ThrottleConfig>(
            "threshold: 1\nwindow_secs: 1\ninternal_metrics: {}\n",
        )
        .unwrap();
        assert!(!config.internal_metrics.include_group_tag);
        assert!(config.internal_metrics.events_discarded_tags.is_none());
        assert!(!config.internal_metrics.emit_events_discarded_per_key);
        assert!(
            serde_json::to_value(&config).unwrap()["internal_metrics"]
                .get("events_discarded_tags")
                .is_none()
        );
        assert!(
            ThrottleConfig::default()
                .internal_metrics
                .events_discarded_tags
                .is_none()
        );
        let without_internal_metrics =
            serde_yaml::from_str::<ThrottleConfig>("threshold: 1\nwindow_secs: 1\n").unwrap();
        assert!(
            without_internal_metrics
                .internal_metrics
                .events_discarded_tags
                .is_none()
        );
    }

    #[test]
    fn internal_metrics_include_group_tag_can_be_enabled() {
        let config = serde_yaml::from_str::<ThrottleConfig>(
            "threshold: 1\nwindow_secs: 1\ninternal_metrics:\n  include_group_tag: true\n",
        )
        .unwrap();
        assert!(config.internal_metrics.include_group_tag);
    }
}
