pub mod v1;
pub mod v2;

use vector_lib::config::{ComponentKey, LogNamespace};
use vector_lib::configurable::configurable_component;

use crate::{
    config::{GenerateConfig, Input, OutputId, TransformConfig, TransformContext, TransformOutput},
    schema,
    transforms::Transform,
};

/// Marker type for the version one of the configuration for the `lua` transform.
#[configurable_component]
#[derive(Clone, Debug)]
enum V1 {
    /// Lua transform API version 1.
    ///
    /// This version is deprecated and will be removed in a future version.
    // TODO: The `deprecated` attribute flag is not used/can't be used for enum values like this
    // because we don't emit the full schema for the enum value, we just gather its description. We
    // might need to consider actually using the flag as a marker to say "append our boilerplate
    // deprecation warning to the description of the field/enum value/etc".
    #[configurable(metadata(deprecated))]
    #[serde(rename = "1")]
    V1,
}

/// Configuration for the version one of the `lua` transform.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct LuaConfigV1 {
    /// Transform API version.
    ///
    /// Specifying this version ensures that backward compatibility is not broken.
    version: Option<V1>,

    #[serde(flatten)]
    config: v1::LuaConfig,
}

/// Marker type for version two of the configuration for the `lua` transform.
#[configurable_component]
#[derive(Clone, Debug)]
enum V2 {
    /// Lua transform API version 2.
    #[serde(rename = "2")]
    V2,
}

/// Configuration for the version two of the `lua` transform.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct LuaConfigV2 {
    /// Transform API version.
    ///
    /// Specifying this version ensures that backward compatibility is not broken.
    version: V2,

    #[serde(flatten)]
    config: v2::LuaConfig,
}

/// Configuration for the `lua` transform.
#[configurable_component(transform(
    "lua",
    "Modify event data using the Lua programming language."
))]
#[derive(Clone, Debug)]
#[serde(untagged)]
pub enum LuaConfig {
    /// Configuration for version one.
    V1(LuaConfigV1),

    /// Configuration for version two.
    V2(LuaConfigV2),
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
        _: vector_lib::enrichment::TableRegistry,
        _: vector_lib::vrl_cache::VrlCacheRegistry,
        input_definitions: &[(OutputId, schema::Definition)],
        _: LogNamespace,
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
}
