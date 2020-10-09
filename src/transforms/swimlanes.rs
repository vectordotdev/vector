use super::Transform;
use crate::{
    conditions::{AnyCondition, Condition},
    config::{DataType, GenerateConfig, TransformConfig, TransformContext, TransformDescription},
    event::Event,
    internal_events::{SwimlanesEventDiscarded, SwimlanesEventProcessed},
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

//------------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct SwimlaneConfig {
    #[serde(flatten)]
    condition: AnyCondition,
}

#[async_trait::async_trait]
#[typetag::serde(name = "swimlane")]
impl TransformConfig for SwimlaneConfig {
    async fn build(&self, _ctx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(Swimlane::new(self.condition.build()?)))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "swimlane"
    }
}

pub struct Swimlane {
    condition: Box<dyn Condition>,
}

impl Swimlane {
    pub fn new(condition: Box<dyn Condition>) -> Self {
        Self { condition }
    }
}

impl Transform for Swimlane {
    fn transform(&mut self, event: Event) -> Option<Event> {
        if self.condition.check(&event) {
            emit!(SwimlanesEventProcessed);
            Some(event)
        } else {
            emit!(SwimlanesEventDiscarded);
            None
        }
    }
}

//------------------------------------------------------------------------------

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct SwimlanesConfig {
    lanes: IndexMap<String, AnyCondition>,
}

inventory::submit! {
    TransformDescription::new::<SwimlanesConfig>("swimlanes")
}

impl GenerateConfig for SwimlanesConfig {}

#[async_trait::async_trait]
#[typetag::serde(name = "swimlanes")]
impl TransformConfig for SwimlanesConfig {
    async fn build(&self, _ctx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        Err("this transform must be expanded".into())
    }

    fn expand(&mut self) -> crate::Result<Option<IndexMap<String, Box<dyn TransformConfig>>>> {
        let mut map: IndexMap<String, Box<dyn TransformConfig>> = IndexMap::new();

        while let Some((k, v)) = self.lanes.pop() {
            map.insert(k.clone(), Box::new(SwimlaneConfig { condition: v }));
        }

        if !map.is_empty() {
            Ok(Some(map))
        } else {
            Err("must specify at least one swimlane".into())
        }
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "swimlanes"
    }
}

//------------------------------------------------------------------------------
