use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use vector_core::transform::SyncTransform;

use crate::{
    conditions::{AnyCondition, Condition},
    config::{
        DataType, GenerateConfig, Input, Output, TransformConfig, TransformContext,
        TransformDescription,
    },
    event::Event,
    schema,
    transforms::Transform,
};

//------------------------------------------------------------------------------

pub(crate) const ELSE_OUTPUT: &str = "_else";

fn build_conditions(
    config: &RouteConfig,
    context: &TransformContext,
) -> crate::Result<Vec<(String, Condition)>> {
    config
        .route
        .iter()
        .map(|(output_name, condition)| {
            condition
                .build(&context.enrichment_tables)
                .map(|condition| (output_name.clone(), condition))
        })
        .collect()
}

//------------------------------------------------------------------------------

#[derive(Clone)]
pub struct EveryMatchRoute {
    conditions: Vec<(String, Condition)>,
}

impl EveryMatchRoute {
    pub fn new(config: &RouteConfig, context: &TransformContext) -> crate::Result<Self> {
        Ok(Self {
            conditions: build_conditions(config, context)?,
        })
    }
}

impl SyncTransform for EveryMatchRoute {
    fn transform(
        &mut self,
        event: Event,
        output: &mut vector_core::transform::TransformOutputsBuf,
    ) {
        let mut discarded = Vec::with_capacity(self.conditions.len());
        for (output_name, condition) in &self.conditions {
            if condition.check(&event) {
                output.push_named(output_name, event.clone());
            } else {
                discarded.push(output_name.as_ref());
            }
        }
        if discarded.len() == self.conditions.len() {
            output.push_named(ELSE_OUTPUT, event);
        }
    }
}

//------------------------------------------------------------------------------

#[derive(Clone)]
pub struct FirstMatchRoute {
    conditions: Vec<(String, Condition)>,
}

impl FirstMatchRoute {
    pub fn new(config: &RouteConfig, context: &TransformContext) -> crate::Result<Self> {
        Ok(Self {
            conditions: build_conditions(config, context)?,
        })
    }
}

impl SyncTransform for FirstMatchRoute {
    fn transform(
        &mut self,
        event: Event,
        output: &mut vector_core::transform::TransformOutputsBuf,
    ) {
        if let Some((output_name, _)) = self
            .conditions
            .iter()
            .find(|(_, condition)| condition.check(&event))
        {
            output.push_named(output_name, event.clone());
        } else {
            output.push_named(ELSE_OUTPUT, event);
        }
    }
}

//------------------------------------------------------------------------------

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RouteMode {
    FirstMatch,
    EveryMatch,
}

// to avoid having the mode field when serializing
impl RouteMode {
    pub const fn is_default(&self) -> bool {
        matches!(self, RouteMode::EveryMatch)
    }
}

impl Default for RouteMode {
    fn default() -> Self {
        RouteMode::EveryMatch
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct RouteConfig {
    // Deprecated name
    #[serde(alias = "lanes")]
    route: IndexMap<String, AnyCondition>,
    #[serde(default, skip_serializing_if = "RouteMode::is_default")]
    mode: RouteMode,
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
            mode: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "route")]
impl TransformConfig for RouteConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        match self.mode {
            RouteMode::FirstMatch => {
                FirstMatchRoute::new(self, context).map(Transform::synchronous)
            }
            RouteMode::EveryMatch => {
                EveryMatchRoute::new(self, context).map(Transform::synchronous)
            }
        }
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn outputs(&self, _: &schema::Definition) -> Vec<Output> {
        let mut result: Vec<Output> = self
            .route
            .keys()
            .map(|output_name| Output::from((output_name, DataType::all())))
            .collect();
        result.push(Output::from((ELSE_OUTPUT, DataType::all())));
        result
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

    fn input(&self) -> Input {
        self.0.input()
    }

    fn outputs(&self, merged_definition: &schema::Definition) -> Vec<Output> {
        self.0.outputs(merged_definition)
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

        let mut transform = EveryMatchRoute::new(&config, &Default::default()).unwrap();
        let mut outputs: Vec<Output> = output_names
            .iter()
            .map(|output_name| Output::from((output_name.to_owned(), DataType::all())))
            .collect();
        outputs.push(Output::default(DataType::all()));
        let mut outputs = TransformOutputsBuf::new_with_capacity(outputs, 1);

        transform.transform(event.clone(), &mut outputs);
        for output_name in output_names {
            let mut events: Vec<_> = outputs.drain_named(output_name).collect();
            assert_eq!(events.len(), 1);
            assert_eq!(events.pop().unwrap(), event);
        }
        let events: Vec<_> = outputs.drain().collect();
        assert!(events.is_empty(), "default output should be empty");
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

        let mut transform = EveryMatchRoute::new(&config, &Default::default()).unwrap();
        let mut outputs: Vec<Output> = output_names
            .iter()
            .map(|output_name| Output::from((output_name.to_owned(), DataType::all())))
            .collect();
        outputs.push(Output::default(DataType::all()));
        let mut outputs = TransformOutputsBuf::new_with_capacity(outputs, 1);

        transform.transform(event.clone(), &mut outputs);
        for output_name in output_names {
            let mut events: Vec<_> = outputs.drain_named(output_name).collect();
            if output_name == "first" {
                assert_eq!(events.len(), 1);
                assert_eq!(events.pop().unwrap(), event);
            }
            assert_eq!(events.len(), 0);
        }
        let events: Vec<_> = outputs.drain().collect();
        assert!(events.is_empty(), "default output should be empty");
    }

    #[test]
    fn route_pass_no_route_condition() {
        let output_names = vec!["first", "second", "third"];
        let event = Event::try_from(serde_json::json!({"message": "foo"})).unwrap();
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

        let mut transform = EveryMatchRoute::new(&config, &Default::default()).unwrap();
        let mut outputs: Vec<Output> = output_names
            .iter()
            .map(|output_name| Output::from((output_name.to_owned(), DataType::all())))
            .collect();
        outputs.push(Output::default(DataType::all()));
        let mut outputs = TransformOutputsBuf::new_with_capacity(outputs, 1);

        transform.transform(event.clone(), &mut outputs);
        for output_name in output_names {
            let events: Vec<_> = outputs.drain_named(output_name).collect();
            assert!(events.is_empty());
        }
        let mut events: Vec<_> = outputs.drain().collect();
        assert_eq!(events.len(), 1);
        assert_eq!(events.pop().unwrap(), event);
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

    #[test]
    fn route_pass_first_route_condition() {
        let output_names = vec!["first", "second", "third"];
        let event = Event::try_from(
            serde_json::json!({"message": "hello world", "second": "second", "third": "third"}),
        )
        .unwrap();
        let config = toml::from_str::<RouteConfig>(
            r#"
            mode = "first_match"

            route.first.type = "vrl"
            route.first.source = '.message == "hello world"'

            route.second.type = "vrl"
            route.second.source = '.second == "second"'

            route.third.type = "vrl"
            route.third.source = '.third == "third"'
        "#,
        )
        .unwrap();

        let mut transform = FirstMatchRoute::new(&config, &Default::default()).unwrap();
        let mut outputs: Vec<Output> = output_names
            .iter()
            .map(|output_name| Output::from((output_name.to_owned(), DataType::all())))
            .collect();
        outputs.push(Output::default(DataType::all()));
        let mut outputs = TransformOutputsBuf::new_with_capacity(outputs, 1);

        transform.transform(event.clone(), &mut outputs);
        for output_name in output_names {
            let mut events: Vec<_> = outputs.drain_named(output_name).collect();
            if output_name == "first" {
                assert_eq!(events.len(), 1);
                assert_eq!(events.pop().unwrap(), event);
            } else {
                assert!(
                    events.is_empty(),
                    "output {:?} should be empty",
                    output_name
                );
            }
        }
        let events: Vec<_> = outputs.drain().collect();
        assert!(events.is_empty(), "default output should be empty");
    }
}
