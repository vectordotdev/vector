use std::{collections::HashMap, future::ready, pin::Pin, time::Duration};

use futures::{Stream, StreamExt};
use vector_lib::{config::LogNamespace, configurable::configurable_component};

use crate::{
    config::{DataType, Input, OutputId, TransformConfig, TransformContext, TransformOutput},
    event::Event,
    schema,
    sinks::util::buffer::metrics::{MetricSet, NormalizerConfig, NormalizerSettings},
    transforms::{TaskTransform, Transform},
};

/// Configuration for the `incremental_to_absolute` transform.
#[configurable_component(transform(
    "incremental_to_absolute",
    "Convert incremental metrics to absolute."
))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct IncrementalToAbsoluteConfig {
    /// Configuration for the internal metrics cache used to normalize a stream of incremental
    /// metrics into absolute metrics.
    ///
    /// By default, incremental metrics are evicted after 5 minutes of not being updated. The next
    /// incremental value will be reset.
    #[configurable(derived)]
    #[serde(default)]
    pub cache: NormalizerConfig<IncrementalToAbsoluteDefaultNormalizerSettings>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct IncrementalToAbsoluteDefaultNormalizerSettings;

impl NormalizerSettings for IncrementalToAbsoluteDefaultNormalizerSettings {
    const MAX_EVENTS: Option<usize> = None;
    const MAX_BYTES: Option<usize> = None;
    const TIME_TO_LIVE: Option<u64> = Some(300);
}

pub const fn default_expire_metrics_secs() -> Duration {
    Duration::from_secs(120)
}

impl_generate_config_from_default!(IncrementalToAbsoluteConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "incremental_to_absolute")]
impl TransformConfig for IncrementalToAbsoluteConfig {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        IncrementalToAbsolute::new(self).map(Transform::event_task)
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn outputs(
        &self,
        _: vector_lib::enrichment::TableRegistry,
        _: &[(OutputId, schema::Definition)],
        _: LogNamespace,
    ) -> Vec<TransformOutput> {
        vec![TransformOutput::new(DataType::Metric, HashMap::new())]
    }
}
#[derive(Debug)]
pub struct IncrementalToAbsolute {
    data: MetricSet,
}

impl IncrementalToAbsolute {
    pub fn new(config: &IncrementalToAbsoluteConfig) -> crate::Result<Self> {
        // Create a new MetricSet with the proper cache settings
        Ok(Self {
            data: MetricSet::new(config.cache.validate()?.into_settings()),
        })
    }
    pub fn transform_one(&mut self, event: Event) -> Option<Event> {
        self.data
            .make_absolute(event.as_metric().clone())
            .map(Event::Metric)
    }
}

impl TaskTransform<Event> for IncrementalToAbsolute {
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
        Metric,
        metric::{MetricKind, MetricValue},
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
    async fn test_incremental_to_absolute() {
        let config = toml::from_str::<IncrementalToAbsoluteConfig>(
            r#"
[cache]
max_events = 100
"#,
        )
        .unwrap();
        let incremental_to_absolute = IncrementalToAbsolute::new(&config)
            .map(Transform::event_task)
            .unwrap();
        let incremental_to_absolute = incremental_to_absolute.into_task();
        let (mut tx, rx) = futures::channel::mpsc::channel(10);
        let mut out_stream = incremental_to_absolute.transform_events(Box::pin(rx));

        let inc_counter_1 = make_metric(
            "incremental_counter",
            MetricKind::Incremental,
            MetricValue::Counter { value: 10.0 },
        );
        let expected_inc_counter_1 = make_metric(
            "incremental_counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 10.0 },
        );
        assert_metric_eq(
            &mut tx,
            &mut out_stream,
            inc_counter_1,
            expected_inc_counter_1,
        )
        .await;

        let inc_counter_2 = make_metric(
            "incremental_counter",
            MetricKind::Incremental,
            MetricValue::Counter { value: 10.0 },
        );
        let expected_inc_counter_2 = make_metric(
            "incremental_counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 20.0 },
        );
        assert_metric_eq(
            &mut tx,
            &mut out_stream,
            inc_counter_2,
            expected_inc_counter_2,
        )
        .await;

        let inc_counter_3 = make_metric(
            "incremental_counter",
            MetricKind::Incremental,
            MetricValue::Counter { value: 10.0 },
        );
        let expected_inc_counter_3 = make_metric(
            "incremental_counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 30.0 },
        );
        assert_metric_eq(
            &mut tx,
            &mut out_stream,
            inc_counter_3,
            expected_inc_counter_3,
        )
        .await;

        // Absolute counters and gauges are emitted unchanged
        let gauge = make_metric(
            "gauge",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 42.0 },
        );
        let expected_gauge = gauge.clone();
        assert_metric_eq(&mut tx, &mut out_stream, gauge, expected_gauge).await;

        let absolute_counter = make_metric(
            "absolute_counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 42.0 },
        );
        let absolute_counter_expected = absolute_counter.clone();
        assert_metric_eq(
            &mut tx,
            &mut out_stream,
            absolute_counter,
            absolute_counter_expected,
        )
        .await;
    }
}
