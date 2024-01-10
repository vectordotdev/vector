use indexmap::IndexMap;
use vector_lib::config::{clone_input_definitions, LogNamespace};
use vector_lib::configurable::configurable_component;
use vector_lib::transform::SyncTransform;

use crate::{
    conditions::{AnyCondition, Condition},
    config::{
        DataType, GenerateConfig, Input, OutputId, TransformConfig, TransformContext,
        TransformOutput,
    },
    event::Event,
    schema,
    transforms::Transform,
};

pub(crate) const UNMATCHED_ROUTE: &str = "_unmatched";

#[derive(Clone)]
pub struct Route {
    conditions: Vec<(String, Condition)>,
    reroute_unmatched: bool,
}

impl Route {
    pub fn new(config: &RouteConfig, context: &TransformContext) -> crate::Result<Self> {
        let mut conditions = Vec::with_capacity(config.route.len());
        for (output_name, condition) in config.route.iter() {
            let condition = condition.build(&context.enrichment_tables)?;
            conditions.push((output_name.clone(), condition));
        }
        Ok(Self {
            conditions,
            reroute_unmatched: config.reroute_unmatched,
        })
    }
}

impl SyncTransform for Route {
    fn transform(&mut self, event: Event, output: &mut vector_lib::transform::TransformOutputsBuf) {
        let mut check_failed: usize = 0;
        for (output_name, condition) in &self.conditions {
            let (result, event) = condition.check(event.clone());
            if result {
                output.push(Some(output_name), event);
            } else {
                check_failed += 1;
            }
        }
        if self.reroute_unmatched && check_failed == self.conditions.len() {
            output.push(Some(UNMATCHED_ROUTE), event);
        }
    }
}

/// Configuration for the `route` transform.
#[configurable_component(transform(
    "route",
    "Split a stream of events into multiple sub-streams based on user-supplied conditions."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct RouteConfig {
    /// Reroutes unmatched events to a named output instead of silently discarding them.
    ///
    /// Normally, if an event doesn't match any defined route, it is sent to the `<transform_name>._unmatched`
    /// output for further processing. In some cases, you may want to simply discard unmatched events and not
    /// process them any further.
    ///
    /// In these cases, `reroute_unmatched` can be set to `false` to disable the `<transform_name>._unmatched`
    /// output and instead silently discard any unmatched events.
    #[serde(default = "crate::serde::default_true")]
    #[configurable(metadata(docs::human_name = "Reroute Unmatched Events"))]
    reroute_unmatched: bool,

    /// A table of route identifiers to logical conditions representing the filter of the route.
    ///
    /// Each route can then be referenced as an input by other components with the name
    /// `<transform_name>.<route_id>`. If an event doesn’t match any route, and if `reroute_unmatched`
    /// is set to `true` (the default), it is sent to the `<transform_name>._unmatched` output.
    /// Otherwise, the unmatched event is instead silently discarded.
    ///
    /// Both `_unmatched`, as well as `_default`, are reserved output names and thus cannot be used
    /// as a route name.
    #[configurable(metadata(docs::additional_props_description = "An individual route."))]
    route: IndexMap<String, AnyCondition>,
}

impl GenerateConfig for RouteConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            reroute_unmatched: true,
            route: IndexMap::new(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "route")]
impl TransformConfig for RouteConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        let route = Route::new(self, context)?;
        Ok(Transform::synchronous(route))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn validate(&self, _: &schema::Definition) -> Result<(), Vec<String>> {
        if self.route.contains_key(UNMATCHED_ROUTE) {
            Err(vec![format!(
                "cannot have a named output with reserved name: `{UNMATCHED_ROUTE}`"
            )])
        } else {
            Ok(())
        }
    }

    fn outputs(
        &self,
        _: vector_lib::enrichment::TableRegistry,
        input_definitions: &[(OutputId, schema::Definition)],
        _: LogNamespace,
    ) -> Vec<TransformOutput> {
        let mut result: Vec<TransformOutput> = self
            .route
            .keys()
            .map(|output_name| {
                TransformOutput::new(DataType::all(), clone_input_definitions(input_definitions))
                    .with_port(output_name)
            })
            .collect();
        if self.reroute_unmatched {
            result.push(
                TransformOutput::new(DataType::all(), clone_input_definitions(input_definitions))
                    .with_port(UNMATCHED_ROUTE),
            );
        }
        result
    }

    fn enable_concurrency(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use indoc::indoc;
    use vector_lib::transform::TransformOutputsBuf;

    use super::*;
    use crate::{
        config::{build_unit_tests, ConfigBuilder},
        test_util::components::{init_test, COMPONENT_MULTIPLE_OUTPUTS_TESTS},
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::RouteConfig>();
    }

    #[test]
    fn can_serialize_remap() {
        // We need to serialize the config to check if a config has
        // changed when reloading.
        let config = toml::from_str::<RouteConfig>(
            r#"
            route.first.type = "vrl"
            route.first.source = '.message == "hello world"'
        "#,
        )
        .unwrap();

        assert_eq!(
            serde_json::to_string(&config).unwrap(),
            r#"{"reroute_unmatched":true,"route":{"first":{"type":"vrl","source":".message == \"hello world\"","runtime":"ast"}}}"#
        );
    }

    #[test]
    fn route_pass_all_route_conditions() {
        let output_names = vec!["first", "second", "third", UNMATCHED_ROUTE];
        let event = Event::from_json_value(
            serde_json::json!({"message": "hello world", "second": "second", "third": "third"}),
            LogNamespace::Legacy,
        )
        .unwrap();
        let config = toml::from_str::<RouteConfig>(
            r#"
            route.first.type = "vrl"
            route.first.source = '.message == "hello world"'

            route.second.type = "vrl"
            route.second.source = '.second == "second"'

            route.third.type = "vrl"
            route.third.source = '.third == "third"'
        "#,
        )
        .unwrap();

        let mut transform = Route::new(&config, &Default::default()).unwrap();
        let mut outputs = TransformOutputsBuf::new_with_capacity(
            output_names
                .iter()
                .map(|output_name| {
                    TransformOutput::new(DataType::all(), HashMap::new())
                        .with_port(output_name.to_owned())
                })
                .collect(),
            1,
        );

        transform.transform(event.clone(), &mut outputs);
        for output_name in output_names {
            let mut events: Vec<_> = outputs.drain_named(output_name).collect();
            if output_name == UNMATCHED_ROUTE {
                assert!(events.is_empty());
            } else {
                assert_eq!(events.len(), 1);
                assert_eq!(events.pop().unwrap(), event);
            }
        }
    }

    #[test]
    fn route_pass_one_route_condition() {
        let output_names = vec!["first", "second", "third", UNMATCHED_ROUTE];
        let event = Event::from_json_value(
            serde_json::json!({"message": "hello world"}),
            LogNamespace::Legacy,
        )
        .unwrap();
        let config = toml::from_str::<RouteConfig>(
            r#"
            route.first.type = "vrl"
            route.first.source = '.message == "hello world"'

            route.second.type = "vrl"
            route.second.source = '.second == "second"'

            route.third.type = "vrl"
            route.third.source = '.third == "third"'
        "#,
        )
        .unwrap();

        let mut transform = Route::new(&config, &Default::default()).unwrap();
        let mut outputs = TransformOutputsBuf::new_with_capacity(
            output_names
                .iter()
                .map(|output_name| {
                    TransformOutput::new(DataType::all(), HashMap::new())
                        .with_port(output_name.to_owned())
                })
                .collect(),
            1,
        );

        transform.transform(event.clone(), &mut outputs);
        for output_name in output_names {
            let mut events: Vec<_> = outputs.drain_named(output_name).collect();
            if output_name == "first" {
                assert_eq!(events.len(), 1);
                assert_eq!(events.pop().unwrap(), event);
            }
            assert_eq!(events.len(), 0);
        }
    }

    #[test]
    fn route_pass_no_route_condition() {
        let output_names = vec!["first", "second", "third", UNMATCHED_ROUTE];
        let event =
            Event::from_json_value(serde_json::json!({"message": "NOPE"}), LogNamespace::Legacy)
                .unwrap();
        let config = toml::from_str::<RouteConfig>(
            r#"
            route.first.type = "vrl"
            route.first.source = '.message == "hello world"'

            route.second.type = "vrl"
            route.second.source = '.second == "second"'

            route.third.type = "vrl"
            route.third.source = '.third == "third"'
        "#,
        )
        .unwrap();

        let mut transform = Route::new(&config, &Default::default()).unwrap();
        let mut outputs = TransformOutputsBuf::new_with_capacity(
            output_names
                .iter()
                .map(|output_name| {
                    TransformOutput::new(DataType::all(), HashMap::new())
                        .with_port(output_name.to_owned())
                })
                .collect(),
            1,
        );

        transform.transform(event.clone(), &mut outputs);
        for output_name in output_names {
            let mut events: Vec<_> = outputs.drain_named(output_name).collect();
            if output_name == UNMATCHED_ROUTE {
                assert_eq!(events.len(), 1);
                assert_eq!(events.pop().unwrap(), event);
            }
            assert_eq!(events.len(), 0);
        }
    }

    #[test]
    fn route_no_unmatched_output() {
        let output_names = vec!["first", "second", "third", UNMATCHED_ROUTE];
        let event =
            Event::from_json_value(serde_json::json!({"message": "NOPE"}), LogNamespace::Legacy)
                .unwrap();
        let config = toml::from_str::<RouteConfig>(
            r#"
            reroute_unmatched = false

            route.first.type = "vrl"
            route.first.source = '.message == "hello world"'

            route.second.type = "vrl"
            route.second.source = '.second == "second"'

            route.third.type = "vrl"
            route.third.source = '.third == "third"'
        "#,
        )
        .unwrap();

        let mut transform = Route::new(&config, &Default::default()).unwrap();
        let mut outputs = TransformOutputsBuf::new_with_capacity(
            output_names
                .iter()
                .map(|output_name| {
                    TransformOutput::new(DataType::all(), HashMap::new())
                        .with_port(output_name.to_owned())
                })
                .collect(),
            1,
        );

        transform.transform(event.clone(), &mut outputs);
        for output_name in output_names {
            let events: Vec<_> = outputs.drain_named(output_name).collect();
            assert_eq!(events.len(), 0);
        }
    }

    #[tokio::test]
    async fn route_metrics_with_output_tag() {
        init_test();

        let config: ConfigBuilder = toml::from_str(indoc! {r#"
            [transforms.foo]
            inputs = []
            type = "route"
            [transforms.foo.route.first]
                type = "is_log"

            [[tests]]
            name = "metric output"

            [tests.input]
                insert_at = "foo"
                value = "none"

            [[tests.outputs]]
                extract_from = "foo.first"
                [[tests.outputs.conditions]]
                type = "vrl"
                source = "true"
        "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert!(tests.remove(0).run().await.errors.is_empty());
        // Check that metrics were emitted with output tag
        COMPONENT_MULTIPLE_OUTPUTS_TESTS.assert(&["output"]);
    }
}
