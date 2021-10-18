use crate::{
    conditions::{AnyCondition, Condition},
    config::{
        DataType, ExpandType, GenerateConfig, TransformConfig, TransformContext,
        TransformDescription,
    },
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
    condition: AnyCondition,
}

#[async_trait::async_trait]
#[typetag::serde(name = "lane")]
impl TransformConfig for LaneConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::function(Lane::new(
            self.condition.build(&context.enrichment_tables)?,
        )))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn output_type(&self) -> DataType {
        DataType::Any
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
            emit!(&RouteEventDiscarded);
        }
    }
}

//------------------------------------------------------------------------------

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct RouteConfig {
    #[serde(flatten)]
    route_or_routes: RouteOrRoutes,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RouteOrRoutes {
    #[serde(alias = "lanes")]
    route: Option<IndexMap<String, AnyCondition>>,
    #[serde(flatten)]
    routes: Option<Routes>,
}

/// Dynamically route to one of the pre-defined routes, based on a field value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Routes {
    /// The field to match against the possible values.
    field: String,

    /// The possible route values of the field.
    ///
    /// When the field matches one of these values, it is sent to the route with a similar name.
    routes: Vec<String>,
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
            route_or_routes: RouteOrRoutes::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "route")]
impl TransformConfig for RouteConfig {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        Err("this transform must be expanded".into())
    }

    fn expand(
        &mut self,
    ) -> crate::Result<Option<(IndexMap<String, Box<dyn TransformConfig>>, ExpandType)>> {
        let mut map: IndexMap<String, Box<dyn TransformConfig>> = IndexMap::new();

        if let Some(ref mut route) = self.route_or_routes.route {
            while let Some((k, v)) = route.pop() {
                if map
                    .insert(k.clone(), Box::new(LaneConfig { condition: v }))
                    .is_some()
                {
                    return Err("duplicate route id".into());
                }
            }
        } else if let Some(ref mut routes) = self.route_or_routes.routes {
            let Routes { field, routes } = routes;

            while let Some(k) = routes.pop() {
                let v = AnyCondition::String(format!("{} == s'{}'", field, k));

                if map
                    .insert(k.clone(), Box::new(LaneConfig { condition: v }))
                    .is_some()
                {
                    return Err("duplicate route id".into());
                }
            }
        } else {
            return Err("Must specify at least one route or dynamic routes".into());
        }

        if !map.is_empty() {
            Ok(Some((map, ExpandType::Parallel)))
        } else {
            Err("must specify at least one lane".into())
        }
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn output_type(&self) -> DataType {
        DataType::Any
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
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        self.0.build(context).await
    }

    fn expand(
        &mut self,
    ) -> crate::Result<Option<(IndexMap<String, Box<dyn TransformConfig>>, ExpandType)>> {
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
    use super::*;

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

    #[test]
    fn can_serialize_remap() {
        // We need to serialize the config to check if a config has
        // changed when reloading.
        let config = LaneConfig {
            condition: AnyCondition::String("foo".to_string()),
        };

        assert_eq!(
            serde_json::to_string(&config).unwrap(),
            r#"{"condition":"foo"}"#
        );
    }

    #[test]
    fn can_serialize_check_fields() {
        // We need to serialize the config to check if a config has
        // changed when reloading.
        let config = toml::from_str::<RouteConfig>(
            r#"
            lanes.first.type = "check_fields"
            lanes.first."message.eq" = "foo"
        "#,
        )
        .unwrap()
        .expand()
        .unwrap()
        .unwrap();

        assert_eq!(
            serde_json::to_string(&config).unwrap(),
            r#"[{"first":{"type":"lane","condition":{"type":"check_fields","message.eq":"foo"}}},"Parallel"]"#
        );
    }
}
