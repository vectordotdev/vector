use serde::{Deserialize, Serialize};

use crate::{
    conditions::{AnyCondition, Condition},
    config::{
        DataType, GenerateConfig, Output, TransformConfig, TransformContext, TransformDescription,
    },
    event::Event,
    internal_events::FilterEventDiscarded,
    transforms::{FunctionTransform, OutputBuffer, Transform},
};

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct FilterConfig {
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

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Any)]
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
    condition: Box<dyn Condition>,
}

impl Filter {
    pub fn new(condition: Box<dyn Condition>) -> Self {
        Self { condition }
    }
}

impl FunctionTransform for Filter {
    fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
        if self.condition.check(&event) {
            output.push(event);
        } else {
            emit!(&FilterEventDiscarded);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        conditions::{is_log::IsLogConfig, ConditionConfig},
        event::Event,
        transforms::test::transform_one,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::FilterConfig>();
    }

    #[test]
    fn passes_metadata() {
        let mut filter = Filter {
            condition: IsLogConfig {}.build(&Default::default()).unwrap(),
        };
        let event = Event::from("message");
        let metadata = event.metadata().clone();
        let result = transform_one(&mut filter, event).unwrap();
        assert_eq!(result.metadata(), &metadata);
    }
}
