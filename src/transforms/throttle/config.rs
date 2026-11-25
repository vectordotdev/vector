use std::time::Duration;

use governor::clock;
use serde_with::serde_as;
use vector_lib::{
    config::clone_input_definitions, configurable::configurable_component,
    lookup::lookup_v2::ConfigTargetPath,
};

use super::transform::Throttle;
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
    /// If true, the counter will be incremented for each discarded event, including the key value
    /// associated with the discarded event. If false, the counter will not be emitted. Instead, the
    /// number of discarded events can be seen through the `component_discarded_events_total` internal
    /// metric.
    ///
    /// Note that this defaults to false because the `key` tag has potentially unbounded cardinality.
    /// Only set this to true if you know that the number of unique keys is bounded.
    #[serde(default)]
    pub emit_events_discarded_per_key: bool,

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

/// Configuration for the `throttle` transform.
#[serde_as]
#[configurable_component(transform("throttle", "Rate limit logs passing through a topology."))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ThrottleConfig {
    /// The default number of events allowed per bucket per `window_secs`.
    ///
    /// A positive threshold is required even when `threshold_field` is set. It applies to
    /// events whose threshold field is missing or invalid.
    pub threshold: u32,

    /// The literal path to the event or metadata field that supplies the threshold for each event.
    ///
    /// Use `rate_limit` for an event field or `%rate_limit` for a metadata field. The value
    /// must be an integer from 1 to 4,294,967,295 or a string that parses as one. Missing
    /// fields and invalid values, including floats, use the configured `threshold`.
    /// Events in the same bucket share capacity even when their thresholds differ.
    #[configurable(metadata(docs::examples = "rate_limit"))]
    pub threshold_field: Option<ConfigTargetPath>,

    /// The time in seconds for a bucket to replenish its full capacity.
    #[serde_as(as = "serde_with::DurationSecondsWithFrac<f64>")]
    #[configurable(metadata(docs::human_name = "Time Window"))]
    pub window_secs: Duration,

    /// A template whose rendered value groups events into independently rate-limited buckets.
    ///
    /// Events use a shared default bucket when the template is omitted or cannot be rendered.
    #[configurable(metadata(docs::examples = "{{ service }}", docs::examples = "{{ hostname }}",))]
    pub key_field: Option<UnconfinedTemplate>,

    /// A condition that lets matching events pass without consuming bucket capacity.
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
            Ok(())
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
    fn internal_metrics_include_group_tag_defaults_to_false() {
        let config = serde_yaml::from_str::<ThrottleConfig>(
            "threshold: 1\nwindow_secs: 1\ninternal_metrics: {}\n",
        )
        .unwrap();
        assert!(!config.internal_metrics.include_group_tag);
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
