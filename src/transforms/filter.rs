use super::Transform;
use crate::{
    conditions::{CheckFieldsConfig, Condition, ConditionConfig},
    event::Event,
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
enum FilterCondition {
    FromType(Box<dyn ConditionConfig>),
    NoTypeCondition(CheckFieldsConfig),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FilterConfig {
    condition: FilterCondition,
}

inventory::submit! {
    TransformDescription::new_without_default::<FilterConfig>("filter")
}

#[typetag::serde(name = "filter")]
impl TransformConfig for FilterConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(Filter::new(match &self.condition {
            FilterCondition::FromType(c) => c.build()?,
            FilterCondition::NoTypeCondition(c) => c.build()?,
        })))
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
        match self.condition.check(&event) {
            true => Some(event),
            false => None,
        }
    }
}
