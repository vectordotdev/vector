use crate::{
    conditions::{AnyCondition, Condition},
    config::{DataType, GenerateConfig, GlobalOptions, TransformConfig, TransformDescription},
    event::Event,
    internal_events::FilterEventDiscarded,
    transforms::{FunctionTransform, Transform},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
struct FilterConfig {
    condition: AnyCondition,
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
    async fn build(&self, _globals: &GlobalOptions) -> crate::Result<Transform> {
        Ok(Transform::function(Filter::new(self.condition.build()?)))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn output_type(&self) -> DataType {
        DataType::Any
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
    fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
        if self.condition.check(&event) {
            output.push(event);
        } else {
            emit!(FilterEventDiscarded);
        }
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::FilterConfig>();
    }
}
