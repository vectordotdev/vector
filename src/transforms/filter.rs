use vector_common::internal_event::{Count, InternalEventHandle as _, Registered};
use vector_config::configurable_component;
use vector_core::config::LogNamespace;

use crate::{
    conditions::{AnyCondition, Condition},
    config::{DataType, GenerateConfig, Input, Output, TransformConfig, TransformContext},
    event::Event,
    internal_events::FilterEventsDropped,
    schema,
    transforms::{FunctionTransform, OutputBuffer, Transform},
};

/// Configuration for the `filter` transform.
#[configurable_component(transform("filter"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FilterConfig {
    #[configurable(derived)]
    /// The condition that every input event is matched against.
    ///
    /// If an event is matched by the condition, it is forwarded. Otherwise, the event is dropped.
    condition: AnyCondition,
}

impl From<AnyCondition> for FilterConfig {
    fn from(condition: AnyCondition) -> Self {
        Self { condition }
    }
}

impl GenerateConfig for FilterConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(r#"condition = ".message = \"value\"""#).unwrap()
    }
}

#[async_trait::async_trait]
impl TransformConfig for FilterConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::function(Filter::new(
            self.condition.build(&context.enrichment_tables)?,
        )))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn outputs(&self, merged_definition: &schema::Definition, _: LogNamespace) -> Vec<Output> {
        vec![Output::default(DataType::all()).with_schema_definition(merged_definition.clone())]
    }

    fn enable_concurrency(&self) -> bool {
        true
    }
}

#[derive(Clone)]
pub struct Filter {
    condition: Condition,
    events_dropped: Registered<FilterEventsDropped>,
}

impl Filter {
    pub fn new(condition: Condition) -> Self {
        Self {
            condition,
            events_dropped: register!(FilterEventsDropped),
        }
    }
}

impl FunctionTransform for Filter {
    fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
        let (result, event) = self.condition.check(event);
        if result {
            output.push(event);
        } else {
            self.events_dropped.emit(Count(1));
        }
    }
}

#[cfg(test)]
mod test {
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;
    use vector_core::event::{Metric, MetricKind, MetricValue};

    use super::*;
    use crate::{
        conditions::ConditionConfig,
        event::{Event, LogEvent},
        test_util::components::assert_transform_compliance,
        transforms::test::create_topology,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::FilterConfig>();
    }

    #[tokio::test]
    async fn filter_basic() {
        assert_transform_compliance(async {
            let transform_config = FilterConfig::from(AnyCondition::from(ConditionConfig::IsLog));

            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), transform_config).await;

            let log = Event::from(LogEvent::from("message"));
            tx.send(log.clone()).await.unwrap();

            assert_eq!(out.recv().await.unwrap(), log);

            let metric = Event::from(Metric::new(
                "test metric",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
            ));
            tx.send(metric).await.unwrap();

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await;
    }
}
