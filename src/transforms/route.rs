use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use vector_core::transform::SyncTransform;

use crate::{
    conditions::{AnyCondition, Condition},
    config::{
        DataType, GenerateConfig, Output, TransformConfig, TransformContext, TransformDescription,
    },
    event::Event,
    internal_events::RouteEventDiscarded,
    transforms::Transform,
};

//------------------------------------------------------------------------------

#[derive(Clone)]
pub struct Route {
    conditions: IndexMap<String, Box<dyn Condition>>,
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
        DataType::all()
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::all())]
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

impl SyncTransform for Route {
    fn transform(
        &mut self,
        event: Event,
        output: &mut vector_core::transform::TransformOutputsBuf,
    ) {
        for (output_name, condition) in self.conditions.iter() {
            if condition.check(&event) {
                output.push_named(output_name, event.clone());
            } else {
                emit!(&RouteEventDiscarded {
                    output: output_name.as_ref()
                });
            }
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
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        let route = Route::new(self, context)?;
        Ok(Transform::synchronous(route))
    }

    fn input_type(&self) -> DataType {
        DataType::all()
    }

    fn outputs(&self) -> Vec<Output> {
        self.route
            .keys()
            .map(|output_name| Output::from((output_name, DataType::all())))
            .collect()
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

    fn input_type(&self) -> DataType {
        self.0.input_type()
    }

    fn outputs(&self) -> Vec<Output> {
        self.0.outputs()
    }

    fn transform_type(&self) -> &'static str {
        self.0.transform_type()
    }
}

//------------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use indoc::indoc;
    use vector_core::transform::TransformOutputsBuf;

    use crate::{
        config::{build_unit_tests, ConfigBuilder},
        test_util::components::{init_test, COMPONENT_MULTIPLE_OUTPUTS_TESTS},
    };

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
        let config = toml::from_str::<RouteConfig>(
            r#"
            route.first.type = "vrl"
            route.first.source = '.message == "hello world"'
        "#,
        )
        .unwrap();

        assert_eq!(
            serde_json::to_string(&config).unwrap(),
            r#"{"route":{"first":{"type":"vrl","source":".message == \"hello world\""}}}"#
        );
    }

    #[test]
    fn can_serialize_check_fields() {
        // We need to serialize the config to check if a config has
        // changed when reloading.
        let config = toml::from_str::<RouteConfig>(
            r#"
            route.first.type = "check_fields"
            route.first."message.eq" = "foo"
        "#,
        )
        .unwrap();

        assert_eq!(
            serde_json::to_string(&config).unwrap(),
            r#"{"route":{"first":{"type":"check_fields","message.eq":"foo"}}}"#
        );
    }

    #[test]
    fn route_pass_all_route_conditions() {
        let output_names = vec!["first", "second", "third"];
        let event = Event::try_from(
            serde_json::json!({"message": "hello world", "second": "second", "third": "third"}),
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
                .map(|output_name| Output::from((output_name.to_owned(), DataType::Any)))
                .collect(),
            1,
        );

        transform.transform(event.clone(), &mut outputs);
        for output_name in output_names {
            let mut events = outputs.drain_named(output_name).collect::<Vec<_>>();
            assert_eq!(events.len(), 1);
            assert_eq!(events.pop().unwrap(), event);
        }
    }

    #[test]
    fn route_pass_one_route_condition() {
        let output_names = vec!["first", "second", "third"];
        let event = Event::try_from(serde_json::json!({"message": "hello world"})).unwrap();
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
                .map(|output_name| Output::from((output_name.to_owned(), DataType::Any)))
                .collect(),
            1,
        );

        transform.transform(event.clone(), &mut outputs);
        for output_name in output_names {
            let mut events = outputs.drain_named(output_name).collect::<Vec<_>>();
            if output_name == "first" {
                assert_eq!(events.len(), 1);
                assert_eq!(events.pop().unwrap(), event);
            }
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
