use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    config::{DataType, ExpandType, Output, TransformConfig, TransformContext},
    event::Event,
    transforms::{FunctionTransform, OutputBuffer, Transform},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum EventType {
    Log,
    Metric,
}

impl Default for EventType {
    fn default() -> Self {
        Self::Log
    }
}

impl EventType {
    const fn validate(&self, event: &Event) -> bool {
        match self {
            Self::Log => matches!(event, Event::Log(_)),
            Self::Metric => matches!(event, Event::Metric(_)),
        }
    }

    const fn data_type(&self) -> DataType {
        match self {
            Self::Log => DataType::Log,
            Self::Metric => DataType::Metric,
        }
    }
}

/// This transform handles a path for a type of event.
/// It expands into a EventFilter that will filter the events depending on their type
/// and then propagate them to the series of pipeline.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventRouterConfig {
    filter: EventType,
    // This inner field contains a list of pipelines that will be expanded.
    inner: Option<Box<dyn TransformConfig>>,
}

impl EventRouterConfig {
    pub fn log(inner: Box<dyn TransformConfig>) -> Self {
        Self {
            filter: EventType::Log,
            inner: Some(inner),
        }
    }

    pub fn metric(inner: Box<dyn TransformConfig>) -> Self {
        Self {
            filter: EventType::Metric,
            inner: Some(inner),
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "pipeline_event_router")]
impl TransformConfig for EventRouterConfig {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        Err("this transform must be expanded".into())
    }

    fn expand(
        &mut self,
    ) -> crate::Result<Option<(IndexMap<String, Box<dyn TransformConfig>>, ExpandType)>> {
        if let Some(ref inner) = self.inner {
            let mut res: IndexMap<String, Box<dyn TransformConfig>> = IndexMap::new();
            res.insert(
                "filter".to_string(),
                Box::new(EventFilterConfig {
                    inner: self.filter.clone(),
                }),
            );
            res.insert("pipelines".to_string(), inner.clone());
            Ok(Some((res, ExpandType::Serial { alias: true })))
        } else {
            Err("must specify at least one pipeline".into())
        }
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(self.filter.data_type())]
    }

    fn transform_type(&self) -> &'static str {
        "pipeline_event_router"
    }
}

/// This transform only filter the events depending on their type.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventFilterConfig {
    inner: EventType,
}

#[async_trait::async_trait]
#[typetag::serde(name = "pipelines_event_router_filter")]
impl TransformConfig for EventFilterConfig {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::function(self.clone()))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(self.inner.data_type())]
    }

    fn transform_type(&self) -> &'static str {
        "pipeline_event_router_filter"
    }
}

impl FunctionTransform for EventFilterConfig {
    fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
        if self.inner.validate(&event) {
            output.push(event);
        }
    }
}
