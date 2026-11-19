use std::{num::NonZeroUsize, time::Duration};

use governor::clock;
use serde_with::serde_as;
use vector_lib::{config::clone_input_definitions, configurable::configurable_component};

use super::transform::Throttle;
use crate::{
    conditions::AnyCondition,
    config::{DataType, Input, OutputId, TransformConfig, TransformContext, TransformOutput},
    schema,
    template::UnconfinedTemplate,
    transforms::{Transform, partitioned::Partitioned},
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
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
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

    /// The number of parallel tasks used to throttle events.
    ///
    /// Buckets are distributed across tasks by hashing their rendered `key_field`, so all events of
    /// a bucket are always rate limited by the same task, in arrival order. Only the order of
    /// output events across different buckets may change.
    ///
    /// Has no effect when `key_field` is unset: all events then share a single bucket, which is
    /// rate limited by a single task. Use at most the number of CPUs, and fewer tasks than distinct
    /// `key_field` values: extra tasks add CPU usage without adding throughput.
    #[serde(default = "default_concurrency")]
    #[derivative(Default(value = "default_concurrency()"))]
    #[configurable(metadata(docs::human_name = "Concurrency"))]
    pub concurrency: NonZeroUsize,
}

impl_generate_config_from_default!(ThrottleConfig);

const fn default_concurrency() -> NonZeroUsize {
    NonZeroUsize::MIN
}

impl ThrottleConfig {
    /// The number of shards to throttle in, or `None` to throttle in a single task: without
    /// `key_field`, all events share one bucket, which only one task can rate limit.
    fn shards(&self) -> Option<NonZeroUsize> {
        Some(self.concurrency).filter(|n| n.get() > 1 && self.key_field.is_some())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "throttle")]
impl TransformConfig for ThrottleConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        let throttle = Throttle::new(self, context, clock::MonotonicClock)?;
        let Some(shards) = self.shards() else {
            return Ok(Transform::event_task(throttle));
        };

        Ok(Transform::event_task(Partitioned::new(
            move || throttle.clone(),
            shards,
        )))
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
    use std::num::NonZeroUsize;

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

    #[test]
    fn concurrency_defaults_to_one_task() {
        let config =
            serde_yaml::from_str::<ThrottleConfig>("threshold: 1\nwindow_secs: 1\n").unwrap();
        assert_eq!(config.concurrency.get(), 1);
        assert_eq!(config.shards(), None);
    }

    #[test]
    fn concurrency_needs_a_key_field_to_shard() {
        let config = serde_yaml::from_str::<ThrottleConfig>(
            "threshold: 1\nwindow_secs: 1\nconcurrency: 4\n",
        )
        .unwrap();
        assert_eq!(config.shards(), None);

        let config = serde_yaml::from_str::<ThrottleConfig>(
            "threshold: 1\nwindow_secs: 1\nconcurrency: 4\nkey_field: \"{{ bucket }}\"\n",
        )
        .unwrap();
        assert_eq!(config.shards().map(NonZeroUsize::get), Some(4));
    }
}
