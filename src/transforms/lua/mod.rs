pub mod v1;
pub mod v2;

use vector_config::configurable_component;

use crate::{
    config::{
        GenerateConfig, Input, Output, TransformConfig, TransformContext, TransformDescription,
    },
    schema,
    transforms::Transform,
};

/// Marker type for the version one of the configuration for the `lua` transform.
#[configurable_component]
#[derive(Clone, Debug)]
enum V1 {
    /// Marker value for version one.
    #[serde(rename = "1")]
    V1,
}

/// Configuration for the version one of the `lua` transform.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct LuaConfigV1 {
    /// Version of the configuration.
    version: Option<V1>,

    #[serde(flatten)]
    config: v1::LuaConfig,
}

/// Marker type for the version two of the configuration for the `lua` transform.
#[configurable_component]
#[derive(Clone, Debug)]
enum V2 {
    /// Marker value for version two.
    #[serde(rename = "2")]
    V2,
}

/// Configuration for the version two of the `lua` transform.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct LuaConfigV2 {
    /// Version of the configuration.
    version: V2,

    #[serde(flatten)]
    config: v2::LuaConfig,
}

/// Configuration for the `lua` transform.
#[configurable_component(transform)]
#[derive(Clone, Debug)]
#[serde(untagged)]
pub enum LuaConfig {
    /// Configuration for version one.
    V1(#[configurable(derived)] LuaConfigV1),

    /// Configuration for version two.
    V2(#[configurable(derived)] LuaConfigV2),
}

inventory::submit! {
    TransformDescription::new::<LuaConfig>("lua")
}

impl GenerateConfig for LuaConfig {
    fn generate_config() -> toml::Value {
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
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        match self {
            LuaConfig::V1(v1) => v1.config.build(),
            LuaConfig::V2(v2) => v2.config.build(),
        }
    }

    fn input(&self) -> Input {
        match self {
            LuaConfig::V1(v1) => v1.config.input(),
            LuaConfig::V2(v2) => v2.config.input(),
        }
    }

    fn outputs(&self, merged_definition: &schema::Definition) -> Vec<Output> {
        match self {
            LuaConfig::V1(v1) => v1.config.outputs(merged_definition),
            LuaConfig::V2(v2) => v2.config.outputs(merged_definition),
        }
    }

    fn transform_type(&self) -> &'static str {
        match self {
            LuaConfig::V1(v1) => v1.config.transform_type(),
            LuaConfig::V2(v2) => v2.config.transform_type(),
        }
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::LuaConfig>();
    }
}
