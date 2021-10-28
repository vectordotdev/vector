use crate::config::{DataType, ExpandType, TransformConfig, TransformContext};
use crate::event::Event;
use crate::transforms::{FunctionTransform, Transform};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

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
    fn validate(&self, event: &Event) -> bool {
        match self {
            Self::Log => matches!(event, Event::Log(_)),
            Self::Metric => matches!(event, Event::Metric(_)),
        }
    }

    fn into_data_type(&self) -> DataType {
        match self {
            Self::Log => DataType::Log,
            Self::Metric => DataType::Metric,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventRouterConfig {
    filter: EventType,
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
            res.insert("transforms".to_string(), inner.clone());
            Ok(Some((res, ExpandType::Serial { alias: true })))
        } else {
            Err("must specify at least one pipeline".into())
        }
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn output_type(&self) -> DataType {
        self.filter.into_data_type()
    }

    fn transform_type(&self) -> &'static str {
        "pipeline_event_router"
    }
}

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

    fn output_type(&self) -> DataType {
        self.inner.into_data_type()
    }

    fn transform_type(&self) -> &'static str {
        "pipeline_event_router_filter"
    }
}

impl FunctionTransform for EventFilterConfig {
    fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
        if self.inner.validate(&event) {
            output.push(event);
        }
    }
}
