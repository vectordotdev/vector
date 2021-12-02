use crate::conditions::{not::NotConfig, AnyCondition, Condition};
use crate::config::{DataType, ExpandType, TransformConfig, TransformContext};
use crate::event::Event;
use crate::transforms::filter::FilterConfig;
use crate::transforms::{DispatchFunctionTransform, Transform};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// This transform is made to do the following trick
///
/// ```text
///                                 +--> filter (if condition) --> ..transforms --+
/// event -- (expand in parallel) --+                                             +--> next pipe
///                                 +--> filter (if !condition) ------------------+
/// ```

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PipelineFilterConfig {
    condition: AnyCondition,
}

impl PipelineFilterConfig {
    pub fn new(condition: AnyCondition) -> Self {
        Self { condition }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "pipeline_filter")]
impl TransformConfig for PipelineFilterConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::DispatchFunction(Box::new(PipelineFilter {
            condition: self.condition.build(&context.enrichment_tables)?,
        })))
    }

    fn named_outputs(&self) -> Vec<(String, DataType)> {
        vec![
            ("truthy".to_owned(), DataType::Any),
            ("falsy".to_owned(), DataType::Any),
        ]
    }

    fn output_type(&self) -> DataType {
        DataType::Any
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn transform_type(&self) -> &'static str {
        "pipeline_filter"
    }
}

#[derive(Clone)]
pub struct PipelineFilter {
    condition: Box<dyn Condition>,
}

impl DispatchFunctionTransform for PipelineFilter {
    fn transform(&mut self, outputs: &mut Vec<(String, Event)>, event: Event) {
        if self.condition.check(&event) {
            outputs.push(("truthy".to_owned(), event));
        } else {
            outputs.push(("falsy".to_owned(), event));
        }
    }
}
