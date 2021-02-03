use crate::{
    conditions::{AnyCondition, Condition},
    config::{DataType, GenerateConfig, TransformConfig, TransformDescription},
    event::Event,
    internal_events::RouteEventDiscarded,
    transforms::{FunctionTransform, Transform},
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

//------------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct LaneConfig {
    #[serde(flatten)]
    condition: AnyCondition,
}

#[async_trait::async_trait]
#[typetag::serde(name = "lane")]
impl TransformConfig for LaneConfig {
    async fn build(&self) -> crate::Result<Transform> {
        Ok(Transform::function(Lane::new(self.condition.build()?)))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "lane"
    }
}

#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct Lane {
    #[derivative(Debug = "ignore")]
    condition: Box<dyn Condition>,
}

impl Lane {
    pub fn new(condition: Box<dyn Condition>) -> Self {
        Self { condition }
    }
}

impl FunctionTransform for Lane {
    fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
        if self.condition.check(&event) {
            output.push(event);
        } else {
            emit!(RouteEventDiscarded);
        }
    }
}

//------------------------------------------------------------------------------

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct RouteConfig {
    // Deprecated name
    #[serde(alias = "lanes")]
    route: IndexMap<String, AnyCondition>,
}

inventory::submit! {
    TransformDescription::new::<RouteConfig>("swimlanes")
}

inventory::submit! {
    TransformDescription::new::<RouteConfig>("route")
}

impl GenerateConfig for RouteConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            route: IndexMap::new(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "route")]
impl TransformConfig for RouteConfig {
    async fn build(&self) -> crate::Result<Transform> {
        Err("this transform must be expanded".into())
    }

    fn expand(&mut self) -> crate::Result<Option<IndexMap<String, Box<dyn TransformConfig>>>> {
        let mut map: IndexMap<String, Box<dyn TransformConfig>> = IndexMap::new();

        while let Some((k, v)) = self.route.pop() {
            map.insert(k.clone(), Box::new(LaneConfig { condition: v }));
        }

        if !map.is_empty() {
            Ok(Some(map))
        } else {
            Err("must specify at least one lane".into())
        }
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "route"
    }
}

// Add a compatibility alias to avoid breaking existing configs
#[derive(Deserialize, Serialize, Debug, Clone)]
struct RouteCompatConfig(RouteConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "swimlanes")]
impl TransformConfig for RouteCompatConfig {
    async fn build(&self) -> crate::Result<Transform> {
        self.0.build().await
    }

    fn expand(&mut self) -> crate::Result<Option<IndexMap<String, Box<dyn TransformConfig>>>> {
        self.0.expand()
    }

    fn input_type(&self) -> DataType {
        self.0.input_type()
    }

    fn output_type(&self) -> DataType {
        self.0.output_type()
    }

    fn transform_type(&self) -> &'static str {
        self.0.transform_type()
    }
}

//------------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use super::RouteConfig;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::RouteConfig>();
    }

    #[test]
    fn alias_works() {
        toml::from_str::<RouteConfig>(
            r#"
            lanes.first.type = "check_fields"
            lanes.first."message.eq" = "foo"
        "#,
        )
        .unwrap();
    }
}
