use crate::conditions::{AnyCondition, ConditionConfig, VrlConfig};
use crate::config::{
    DataType, GenerateConfig, Input, LogNamespace, OutputId, TransformConfig, TransformContext,
    TransformOutput,
};
use crate::schema;
use crate::sinks::prelude::configurable_component;
use crate::transforms::exclusive_route::transform::ExclusiveRoute;
use crate::transforms::Transform;
use std::hash::{Hash, Hasher};
use vector_lib::config::clone_input_definitions;

pub(super) const UNMATCHED_ROUTE: &str = "_unmatched";

/// Individual route configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct Route {
    /// The name of the route is also the name of the transform port.
    ///
    ///  The `_unmatched` name is reserved and thus cannot be used as route ID.
    ///
    /// Each route can then be referenced as an input by other components with the name
    ///  `<transform_name>.<name>`. If an event doesn’t match any route,
    /// it is sent to the `<transform_name>._unmatched` output.
    pub name: String,

    /// Each condition represents a filter which is applied to each event.
    pub condition: AnyCondition,
}

impl Hash for Route {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl PartialEq for Route {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for Route {}

/// Configuration for the `route` transform.
#[configurable_component(transform(
    "exclusive_route",
    "Split a stream of events into unique sub-streams based on user-supplied conditions."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ExclusiveRouteConfig {
    /// An array of named routes. The route names are expected to be unique.
    #[configurable(metadata(docs::examples = "routes_example()"))]
    pub routes: Vec<Route>,
}

fn routes_example() -> Vec<Route> {
    vec![
        Route {
            name: "foo-and-bar-exist".to_owned(),
            condition: AnyCondition::Map(ConditionConfig::Vrl(VrlConfig {
                source: "exists(.foo) && exists(.bar)".to_owned(),
                ..Default::default()
            })),
        },
        Route {
            name: "only-foo-exists".to_owned(),
            condition: AnyCondition::Map(ConditionConfig::Vrl(VrlConfig {
                source: "exists(.foo)".to_owned(),
                ..Default::default()
            })),
        },
    ]
}

impl GenerateConfig for ExclusiveRouteConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            routes: routes_example(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "exclusive_route")]
impl TransformConfig for ExclusiveRouteConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        let route = ExclusiveRoute::new(self, context)?;
        Ok(Transform::synchronous(route))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn validate(&self, _: &schema::Definition) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        let mut counts = std::collections::HashMap::new();
        for route in &self.routes {
            *counts.entry(route.name.clone()).or_insert(0) += 1;
        }

        let duplicates: Vec<String> = counts
            .iter()
            .filter(|&(_, &count)| count > 1)
            .map(|(name, _)| name.clone())
            .collect();

        if !duplicates.is_empty() {
            errors.push(format!("Found routes with duplicate names: {duplicates:?}"));
        }

        if self
            .routes
            .iter()
            .any(|route| route.name == UNMATCHED_ROUTE)
        {
            errors.push(format!("Using reserved '{UNMATCHED_ROUTE}' name."));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn outputs(
        &self,
        _: vector_lib::enrichment::TableRegistry,
        input_definitions: &[(OutputId, schema::Definition)],
        _: LogNamespace,
    ) -> Vec<TransformOutput> {
        let mut outputs: Vec<_> = self
            .routes
            .iter()
            .map(|route| {
                TransformOutput::new(
                    DataType::all_bits(),
                    clone_input_definitions(input_definitions),
                )
                .with_port(route.name.clone())
            })
            .collect();
        outputs.push(
            TransformOutput::new(
                DataType::all_bits(),
                clone_input_definitions(input_definitions),
            )
            .with_port(UNMATCHED_ROUTE),
        );
        outputs
    }

    fn enable_concurrency(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::ExclusiveRouteConfig;
    use indoc::indoc;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<ExclusiveRouteConfig>();
    }

    #[test]
    fn can_serialize_remap() {
        // We need to serialize the config to check if a config has
        // changed when reloading.
        let config = serde_yaml::from_str::<ExclusiveRouteConfig>(indoc! {r#"
                routes:
                    - name: a
                      condition:
                        type = "vrl"
                        source = '.message == "hello world"'
            "#})
        .unwrap();

        assert_eq!(
            serde_json::to_string(&config).unwrap(),
            r#"{"routes":[{"name":"a","condition":"type = \"vrl\" source = '.message == \"hello world\"'"}]}"#
        );
    }
}
