use std::{collections::HashMap, future::ready, pin::Pin, time::Duration};

use futures::{Stream, StreamExt};
use vector_lib::configurable::configurable_component;

use crate::{
    config::{DataType, Input, OutputId, TransformConfig, TransformContext, TransformOutput},
    event::Event,
    schema,
    sinks::util::buffer::metrics::{MetricSet, NormalizerConfig, NormalizerSettings},
    transforms::{TaskTransform, Transform},
};

/// Configuration for the `absolute_to_incremental` transform.
#[configurable_component(transform(
    "absolute_to_incremental",
    "Convert absolute metrics to incremental."
))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct AbsoluteToIncrementalConfig {
    /// Configuration for the internal metrics cache used to normalize a stream of absolute
    /// metrics into incremental metrics.
    ///
    /// By default, absolute metrics are evicted after 5 minutes of not being updated. The next
    /// absolute value will be reset.
    #[configurable(derived)]
    #[serde(default)]
    pub cache: NormalizerConfig<AbsoluteToIncrementalDefaultNormalizerSettings>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AbsoluteToIncrementalDefaultNormalizerSettings;

impl NormalizerSettings for AbsoluteToIncrementalDefaultNormalizerSettings {
    const MAX_EVENTS: Option<usize> = None;
    const MAX_BYTES: Option<usize> = None;
    const TIME_TO_LIVE: Option<u64> = Some(300);
}

pub const fn default_expire_metrics_secs() -> Duration {
    Duration::from_secs(120)
}

impl_generate_config_from_default!(AbsoluteToIncrementalConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "absolute_to_incremental")]
impl TransformConfig for AbsoluteToIncrementalConfig {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        AbsoluteToIncremental::new(self).map(Transform::event_task)
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

#[derive(Debug)]
pub struct AbsoluteToIncremental {
    data: MetricSet,
}

impl AbsoluteToIncremental {
    pub fn new(config: &AbsoluteToIncrementalConfig) -> crate::Result<Self> {
        // Create a new MetricSet with the proper cache settings
        Ok(Self {
            data: MetricSet::new(config.cache.validate()?.into_settings()),
        })
    }

    pub fn transform_one(&mut self, event: Event) -> Option<Event> {
        self.data
            .make_incremental(event.as_metric().clone())
            .map(Event::Metric)
    }
}

impl TaskTransform<Event> for AbsoluteToIncremental {
    fn transform(
        self: Box<Self>,
        task: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static,
    {
        let mut inner = self;
        Box::pin(task.filter_map(move |v| ready(inner.transform_one(v))))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures_util::SinkExt;
    use similar_asserts::assert_eq;
    use vector_lib::config::ComponentKey;

    use super::*;
    use crate::event::{
        metric::{MetricKind, MetricValue},
        Metric,
    };

    fn make_metric(name: &'static str, kind: MetricKind, value: MetricValue) -> Event {
        let mut event = Event::Metric(Metric::new(name, kind, value))
            .with_source_id(Arc::new(ComponentKey::from("in")))
            .with_upstream_id(Arc::new(OutputId::from("transform")));

        event.metadata_mut().set_source_type("unit_test_stream");

        event
    }

    async fn assert_metric_eq(
        tx: &mut futures::channel::mpsc::Sender<Event>,
        mut out_stream: impl Stream<Item = Event> + Unpin,
        metric: Event,
        expected_metric: Event,
    ) {
        tx.send(metric).await.unwrap();
        if let Some(out_event) = out_stream.next().await {
            let result = out_event;
            assert_eq!(result, expected_metric);
        } else {
            panic!("Unexpectedly received None in output stream");
        }
    }

    #[tokio::test]
    async fn test_absolute_to_incremental() {
        let config = toml::from_str::<AbsoluteToIncrementalConfig>(
            r#"
[cache]
max_events = 100
"#,
        )
        .unwrap();
        let absolute_to_incremental = AbsoluteToIncremental::new(&config)
            .map(Transform::event_task)
            .unwrap();
        let absolute_to_incremental = absolute_to_incremental.into_task();
        let (mut tx, rx) = futures::channel::mpsc::channel(10);
        let mut out_stream = absolute_to_incremental.transform_events(Box::pin(rx));

        let abs_counter_0 = make_metric(
            "absolute_counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 0.0 },
        );
        tx.send(abs_counter_0).await.unwrap();

        let abs_counter_1 = make_metric(
            "absolute_counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 10.0 },
        );
        let expected_abs_counter_1 = make_metric(
            "absolute_counter",
            MetricKind::Incremental,
            MetricValue::Counter { value: 10.0 },
        );
        assert_metric_eq(
            &mut tx,
            &mut out_stream,
            abs_counter_1,
            expected_abs_counter_1,
        )
        .await;

        let abs_counter_2 = make_metric(
            "absolute_counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 20.0 },
        );
        let expected_abs_counter_2 = make_metric(
            "absolute_counter",
            MetricKind::Incremental,
            MetricValue::Counter { value: 10.0 },
        );
        assert_metric_eq(
            &mut tx,
            &mut out_stream,
            abs_counter_2,
            expected_abs_counter_2,
        )
        .await;

        let abs_counter_3 = make_metric(
            "absolute_counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 30.0 },
        );
        let expected_abs_counter_3 = make_metric(
            "absolute_counter",
            MetricKind::Incremental,
            MetricValue::Counter { value: 10.0 },
        );
        assert_metric_eq(
            &mut tx,
            &mut out_stream,
            abs_counter_3,
            expected_abs_counter_3,
        )
        .await;

        // Incremental counters are emitted unchanged
        let incremental_counter = make_metric(
            "incremental_counter",
            MetricKind::Incremental,
            MetricValue::Counter { value: 42.0 },
        );
        let expected_incremental_counter = incremental_counter.clone();
        assert_metric_eq(
            &mut tx,
            &mut out_stream,
            incremental_counter,
            expected_incremental_counter,
        )
        .await;
    }
}
