use super::Transform;
use crate::{
    conditions::{Condition, ConditionConfig},
    event::Event,
    runtime::TaskExecutor,
    topology::config::{DataType, TransformConfig, TransformDescription},
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

//------------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum SwimlaneConditionConfig {
    String(String),
    Embedded(Box<dyn ConditionConfig>),
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct SwimlaneConfig {
    #[serde(skip)]
    name: String,

    #[serde(flatten)]
    condition: SwimlaneConditionConfig,
}

#[typetag::serde(name = "swimlane")]
impl TransformConfig for SwimlaneConfig {
    fn build(&self, _exec: TaskExecutor) -> crate::Result<Box<dyn Transform>> {
        let cond = match &self.condition {
            SwimlaneConditionConfig::Embedded(b) => b.build()?,
            SwimlaneConditionConfig::String(_s) => {
                return Err(format!(
                    "failed to create swimlane '{}': condition references are not yet supported",
                    self.name,
                )
                .into());
            }
        };
        Ok(Box::new(Swimlane::new(cond)))
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
            Some(event)
        } else {
            None
        }
    }
}

//------------------------------------------------------------------------------

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct SwimlanesConfig {
    lanes: IndexMap<String, SwimlaneConditionConfig>,
}

inventory::submit! {
    TransformDescription::new_without_default::<SwimlanesConfig>("swimlanes")
}

#[typetag::serde(name = "swimlanes")]
impl TransformConfig for SwimlanesConfig {
    fn build(&self, _exec: TaskExecutor) -> crate::Result<Box<dyn Transform>> {
        Err("this transform must be expanded".into())
    }

    fn expand(&mut self) -> Option<IndexMap<String, Box<dyn TransformConfig>>> {
        let mut map: IndexMap<String, Box<dyn TransformConfig>> = IndexMap::new();

        while let Some((k, v)) = self.lanes.pop() {
            map.insert(
                k.clone(),
                Box::new(SwimlaneConfig {
                    name: k,
                    condition: v,
                }),
            );
        }

        if !map.is_empty() {
            Some(map)
        } else {
            None
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
