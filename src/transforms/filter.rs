use super::Transform;
use crate::{
    conditions::{AnyCondition, Condition},
    config::{DataType, GenerateConfig, TransformConfig, TransformDescription},
    event::Event,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
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
    async fn build(&self) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(Filter::new(self.condition.build()?)))
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

pub struct Filter {
    condition: Box<dyn Condition>,
}

impl Filter {
    pub fn new(condition: Box<dyn Condition>) -> Self {
        Self { condition }
    }
}

impl Transform for Filter {
    fn transform(&mut self, event: Event) -> Option<Event> {
        if self.condition.check(&event) {
            Some(event)
        } else {
            None
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
