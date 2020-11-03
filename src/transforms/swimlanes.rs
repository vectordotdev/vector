use crate::{
    conditions::{AnyCondition, Condition},
    config::{DataType, GenerateConfig, TransformConfig, TransformDescription},
    event::Event,
    internal_events::{SwimlanesEventDiscarded, SwimlanesEventProcessed},
    transforms::{FunctionTransform, Transform},
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

//------------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SwimlaneConfig {
    #[serde(flatten)]
    condition: AnyCondition,
}

#[async_trait::async_trait]
#[typetag::serde(name = "swimlane")]
impl TransformConfig for SwimlaneConfig {
    async fn build(&self) -> crate::Result<Transform> {
        Ok(Transform::function(Swimlane::new(self.condition.build()?)))
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

#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct Swimlane {
    #[derivative(Debug = "ignore")]
    condition: Box<dyn Condition>,
}

impl Swimlane {
    pub fn new(condition: Box<dyn Condition>) -> Self {
        Self { condition }
    }
}

impl FunctionTransform for Swimlane {
    fn transform(&self, output: &mut Vec<Event>, event: Event) {
        if self.condition.check(&event) {
            emit!(SwimlanesEventProcessed);
            output.push(event);
        } else {
            emit!(SwimlanesEventDiscarded);
        }
    }
}

//------------------------------------------------------------------------------

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SwimlanesConfig {
    lanes: IndexMap<String, AnyCondition>,
}

inventory::submit! {
    TransformDescription::new::<SwimlanesConfig>("swimlanes")
}

impl GenerateConfig for SwimlanesConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            lanes: IndexMap::new(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "swimlanes")]
impl TransformConfig for SwimlanesConfig {
    async fn build(&self) -> crate::Result<Transform> {
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

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::SwimlanesConfig>();
    }
}
