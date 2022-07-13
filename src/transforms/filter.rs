use std::time::{Duration, Instant};

use vector_config::configurable_component;

use crate::{
    conditions::{AnyCondition, Condition},
    config::{
        DataType, GenerateConfig, Input, Output, TransformConfig, TransformContext,
        TransformDescription,
    },
    event::Event,
    internal_events::FilterEventDiscarded,
    schema,
    transforms::{FunctionTransform, OutputBuffer, Transform},
};

/// Configuration for the `filter` transform.
#[configurable_component(transform)]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FilterConfig {
    #[configurable(derived)]
    condition: AnyCondition,
}

impl From<AnyCondition> for FilterConfig {
    fn from(condition: AnyCondition) -> Self {
        Self { condition }
    }
}

inventory::submit! {
    TransformDescription::new::<FilterConfig>("filter")
}

impl GenerateConfig for FilterConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"condition.type = "check_fields"
            condition."message.eq" = "value""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "filter")]
impl TransformConfig for FilterConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::function(Filter::new(
            self.condition.build(&context.enrichment_tables)?,
        )))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn outputs(&self, _: &schema::Definition) -> Vec<Output> {
        vec![Output::default(DataType::all())]
    }

    fn enable_concurrency(&self) -> bool {
        true
    }

    fn transform_type(&self) -> &'static str {
        "filter"
    }
}

#[derive(Derivative, Clone)]
#[derivative(Debug)]
pub struct Filter {
    #[derivative(Debug = "ignore")]
    condition: Condition,
    last_emission: Instant,
    emissions_max_delay: Duration,
    emissions_deferred: u64,
}

impl Filter {
    pub fn new(condition: Condition) -> Self {
        Self {
            condition,
            last_emission: Instant::now(),
            emissions_max_delay: Duration::new(2, 0),
            emissions_deferred: 0,
        }
    }
}

impl FunctionTransform for Filter {
    fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
        let (result, event) = self.condition.check(event);
        if result {
            output.push(event);
        } else if self.last_emission.elapsed() >= self.emissions_max_delay {
            emit!(FilterEventDiscarded {
                total: self.emissions_deferred,
            });
            self.emissions_deferred = 0;
            self.last_emission = Instant::now();
        } else {
            self.emissions_deferred += 1;
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::event::{Event, LogEvent};
    use crate::{conditions::Condition, transforms::test::transform_one};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::FilterConfig>();
    }

    #[test]
    fn passes_metadata() {
        let mut filter = Filter::new(Condition::IsLog);
        let event = Event::from(LogEvent::from("message"));
        let metadata = event.metadata().clone();
        let result = transform_one(&mut filter, event).unwrap();
        assert_eq!(result.metadata(), &metadata);
    }
}
