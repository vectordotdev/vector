use crate::config::{DataType, ExpandType, TransformConfig, TransformContext};
use crate::event::Event;
use crate::transforms::{DispatchFunctionTransform, FunctionTransform, Transform};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use vector_core::config::ComponentKey;

/// This transform handles a path for a type of event.
/// It expands into a EventFilter that will filter the events depending on their type
/// and then propagate them to the series of pipeline.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct EventRouterConfig;

#[async_trait::async_trait]
#[typetag::serde(name = "pipeline_event_router")]
impl TransformConfig for EventRouterConfig {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::DispatchFunction(
            Box::new(EventRouter::default()),
        ))
    }

    fn named_outputs(&self) -> Vec<(String, DataType)> {
        vec![
            ("logs".to_owned(), DataType::Log),
            ("metrics".to_owned(), DataType::Metric),
        ]
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn output_type(&self) -> DataType {
        DataType::Any
    }

    fn transform_type(&self) -> &'static str {
        "pipeline_event_router"
    }
}

#[derive(Clone, Default)]
pub struct EventRouter;

impl DispatchFunctionTransform for EventRouter {
    fn transform(&mut self, output: &mut Vec<(String, Event)>, event: Event) {
        let name = match event {
            Event::Log(_) => "logs".to_owned(),
            Event::Metric(_) => "metrics".to_owned(),
        };
        output.push((name, event));
    }
}
