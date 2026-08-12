pub mod v1;
pub mod v2;

use vector_lib::{config::ComponentKey, configurable::configurable_component};

use crate::{
    config::{GenerateConfig, Input, OutputId, TransformConfig, TransformContext, TransformOutput},
    schema,
    transforms::Transform,
};

/// Configuration for the version one of the `lua` transform.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct LuaConfigV1 {
    #[serde(flatten)]
    config: v1::LuaConfig,
}

/// Configuration for the version two of the `lua` transform.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct LuaConfigV2 {
    #[serde(flatten)]
    config: v2::LuaConfig,
}

/// Configuration for the `lua` transform.
#[configurable_component(transform(
    "lua",
    "Modify event data using the Lua programming language."
))]
#[derive(Clone, Debug)]
#[serde(tag = "version")]
#[configurable(metadata(
    docs::enum_tag_description = "Transform API version. Specifying this version ensures that backward compatibility is not broken."
))]
pub enum LuaConfig {
    /// Configuration for version two.
    #[serde(rename = "2")]
    V2(LuaConfigV2),

    /// Configuration for version one.
    ///
    /// This version is deprecated and will be removed in a future version.
    #[configurable(metadata(deprecated))]
    #[serde(rename = "1")]
    V1(LuaConfigV1),
}

impl GenerateConfig for LuaConfig {
    fn generate_config() -> serde_json::Value {
        toml::from_str(
            r#"version = "2"
            hooks.process = """#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "lua")]
impl TransformConfig for LuaConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        let key = context
            .key
            .as_ref()
            .map_or_else(|| ComponentKey::from("lua"), Clone::clone);
        match self {
            LuaConfig::V1(v1) => v1.config.build(),
            LuaConfig::V2(v2) => v2.config.build(key),
        }
    }

    fn input(&self) -> Input {
        match self {
            LuaConfig::V1(v1) => v1.config.input(),
            LuaConfig::V2(v2) => v2.config.input(),
        }
    }

    fn outputs(
        &self,
        _: &TransformContext,
        input_definitions: &[(OutputId, schema::Definition)],
    ) -> Vec<TransformOutput> {
        match self {
            LuaConfig::V1(v1) => v1.config.outputs(input_definitions),
            LuaConfig::V2(v2) => v2.config.outputs(input_definitions),
        }
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::LuaConfig>();
    }

    #[test]
    fn rejects_auto_metric_tag_values() {
        assert!(
            serde_yaml::from_str::<super::LuaConfig>(indoc::indoc! {r#"
                version: "2"
                metric_tag_values: auto
                hooks:
                  process: |
                    function (event, emit)
                      emit(event)
                    end
            "#})
            .is_err(),
            "metric_tag_values = auto must be rejected at parse time"
        );
    }

    #[test]
    fn version_is_required() {
        // The `version` field is the enum tag and must always be specified.
        assert!(
            serde_yaml::from_str::<super::LuaConfig>(indoc::indoc! {r#"
                source: |
                  event["a"] = "b"
            "#})
            .is_err(),
            "a config without `version` must be rejected"
        );
    }

    #[test]
    fn version_dispatches_to_the_correct_config() {
        let v1 = serde_yaml::from_str::<super::LuaConfig>(indoc::indoc! {r#"
            version: "1"
            source: |
              event["a"] = "b"
        "#})
        .unwrap();
        assert!(matches!(v1, super::LuaConfig::V1(_)));

        let v2 = serde_yaml::from_str::<super::LuaConfig>(indoc::indoc! {r#"
            version: "2"
            hooks:
              process: |
                function (event, emit)
                  emit(event)
                end
        "#})
        .unwrap();
        assert!(matches!(v2, super::LuaConfig::V2(_)));
    }

    #[test]
    fn rejects_unknown_fields() {
        // `deny_unknown_fields` must still be enforced on the versioned configs.
        assert!(
            serde_yaml::from_str::<super::LuaConfig>(indoc::indoc! {r#"
                version: "2"
                hooks:
                  process: |
                    function (event, emit)
                      emit(event)
                    end
                unknown_field: true
            "#})
            .is_err(),
            "unknown fields must be rejected"
        );
    }
}
