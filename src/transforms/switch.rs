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

#[derive(Clone)]
pub struct Switch {
    conditions: Vec<(String, Condition)>,
}

impl Switch {
    pub fn new(config: &SwitchConfig, context: &TransformContext) -> crate::Result<Self> {
        let mut conditions = Vec::with_capacity(config.cases.len());
        for (idx, condition) in config.cases.iter().enumerate() {
            let condition = condition.build(&context.enrichment_tables)?;
            let output_name = format!("case_{}", idx);
            conditions.push((output_name, condition));
        }
        Ok(Self { conditions })
    }
}

impl SyncTransform for Switch {
    fn transform(
        &mut self,
        event: Event,
        output: &mut vector_core::transform::TransformOutputsBuf,
    ) {
        let output_name = self
            .conditions
            .iter()
            .filter(|(_, condition)| condition.check(&event))
            .map(|(name, _)| name)
            .next();
        if let Some(name) = output_name {
            output.push_named(name, event);
        } else {
            output.push_named("default", event);
        }
    }
}

//------------------------------------------------------------------------------

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SwitchConfig {
    cases: Vec<AnyCondition>,
}

inventory::submit! {
    TransformDescription::new::<SwitchConfig>("route")
}

impl GenerateConfig for SwitchConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self { cases: Vec::new() }).unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "switch")]
impl TransformConfig for SwitchConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        let route = Switch::new(self, context)?;
        Ok(Transform::synchronous(route))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn outputs(&self, _: &schema::Definition) -> Vec<Output> {
        self.cases
            .iter()
            .enumerate()
            .map(|(idx, _)| format!("case_{}", idx))
            .map(|output_name| Output::from((output_name, DataType::all())))
            .collect()
    }

    fn transform_type(&self) -> &'static str {
        "switch"
    }
}

//------------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use super::*;
    use vector_core::transform::TransformOutputsBuf;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::SwitchConfig>();
    }

    #[test]
    fn can_serialize_remap() {
        // We need to serialize the config to check if a config has
        // changed when reloading.
        let config = toml::from_str::<SwitchConfig>(
            r#"
            [[cases]]
            type = "vrl"
            source = '.message == "hello world"'
        "#,
        )
        .unwrap();

        assert_eq!(
            serde_json::to_string(&config).unwrap(),
            r#"{"cases":[{"type":"vrl","source":".message == \"hello world\""}]}"#
        );
    }

    #[test]
    fn can_serialize_check_fields() {
        // We need to serialize the config to check if a config has
        // changed when reloading.
        let config = toml::from_str::<SwitchConfig>(
            r#"
            [[cases]]
            type = "check_fields"
            "message.eq" = "foo"
        "#,
        )
        .unwrap();

        assert_eq!(
            serde_json::to_string(&config).unwrap(),
            r#"{"cases":[{"type":"check_fields","message.eq":"foo"}]}"#
        );
    }

    #[test]
    fn pass_no_route_conditions() {
        let output_names = vec!["case_0", "case_1", "case_2", "default"];
        let event = Event::try_from(serde_json::json!({"message": "noop"})).unwrap();
        let config = toml::from_str::<SwitchConfig>(
            r#"
            [[cases]]
            type = "vrl"
            source = '.message == "hello world"'

            [[cases]]
            type = "vrl"
            source = '.second == "second"'

            [[cases]]
            type = "vrl"
            source = '.third == "third"'
        "#,
        )
        .unwrap();

        let mut transform = Switch::new(&config, &Default::default()).unwrap();
        let mut outputs = TransformOutputsBuf::new_with_capacity(
            output_names
                .iter()
                .map(|output_name| Output::from((output_name.to_owned(), DataType::all())))
                .collect(),
            1,
        );

        transform.transform(event.clone(), &mut outputs);
        for output_name in output_names {
            let mut events: Vec<_> = outputs.drain_named(output_name).collect();
            if output_name == "default" {
                assert_eq!(events.len(), 1);
                assert_eq!(events.pop().unwrap(), event);
            }
            assert_eq!(events.len(), 0);
        }
    }

    #[test]
    fn pass_one_route_condition() {
        let output_names = vec!["case_0", "case_1", "case_2", "default"];
        let event = Event::try_from(serde_json::json!({"message": "hello world"})).unwrap();
        let config = toml::from_str::<SwitchConfig>(
            r#"
            [[cases]]
            type = "vrl"
            source = '.message == "hello world"'

            [[cases]]
            type = "vrl"
            source = '.second == "second"'

            [[cases]]
            type = "vrl"
            source = '.third == "third"'
        "#,
        )
        .unwrap();

        let mut transform = Switch::new(&config, &Default::default()).unwrap();
        let mut outputs = TransformOutputsBuf::new_with_capacity(
            output_names
                .iter()
                .map(|output_name| Output::from((output_name.to_owned(), DataType::all())))
                .collect(),
            1,
        );

        transform.transform(event.clone(), &mut outputs);
        for output_name in output_names {
            let mut events: Vec<_> = outputs.drain_named(output_name).collect();
            if output_name == "case_0" {
                assert_eq!(events.len(), 1);
                assert_eq!(events.pop().unwrap(), event);
            }
            assert_eq!(events.len(), 0);
        }
    }
}
