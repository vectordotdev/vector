pub mod v1;
pub mod v2;

use vector_config::configurable_component;

use crate::config::{
    GenerateConfig, Output, Resource, SourceConfig, SourceContext, SourceDescription,
};

/// Marker type for the version one of the configuration for the `vector` source.
#[configurable_component]
#[derive(Clone, Debug)]
enum V1 {
    /// Marker value for version one.
    #[serde(rename = "1")]
    V1,
}

/// Configuration for version two of the `vector` source.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct VectorConfigV1 {
    /// Version of the configuration.
    version: V1,

    #[serde(flatten)]
    config: v1::VectorConfig,
}

/// Marker type for the version two of the configuration for the `vector` source.
#[configurable_component]
#[derive(Clone, Debug)]
enum V2 {
    /// Marker value for version two.
    #[serde(rename = "2")]
    V2,
}

/// Configuration for version two of the `vector` source.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct VectorConfigV2 {
    /// Version of the configuration.
    version: Option<V2>,

    #[serde(flatten)]
    config: v2::VectorConfig,
}

/// Configurable for the `vector` source.
#[configurable_component(source)]
#[derive(Clone, Debug)]
#[serde(untagged)]
pub enum VectorConfig {
    /// Configuration for version one.
    V1(#[configurable(derived)] VectorConfigV1),

    /// Configuration for version two.
    V2(#[configurable(derived)] VectorConfigV2),
}

inventory::submit! {
    SourceDescription::new::<VectorConfig>("vector")
}

impl GenerateConfig for VectorConfig {
    fn generate_config() -> toml::Value {
        let config =
            toml::Value::try_into::<v2::VectorConfig>(v2::VectorConfig::generate_config()).unwrap();
        toml::Value::try_from(VectorConfigV2 {
            version: Some(V2::V2),
            config,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "vector")]
impl SourceConfig for VectorConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        match self {
            VectorConfig::V1(v1) => v1.config.build(cx).await,
            VectorConfig::V2(v2) => v2.config.build(cx).await,
        }
    }

    fn outputs(&self) -> Vec<Output> {
        match self {
            VectorConfig::V1(v1) => v1.config.outputs(),
            VectorConfig::V2(v2) => v2.config.outputs(),
        }
    }

    fn source_type(&self) -> &'static str {
        match self {
            VectorConfig::V1(v1) => v1.config.source_type(),
            VectorConfig::V2(v2) => v2.config.source_type(),
        }
    }

    fn resources(&self) -> Vec<Resource> {
        match self {
            VectorConfig::V1(v1) => v1.config.resources(),
            VectorConfig::V2(v2) => v2.config.resources(),
        }
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::VectorConfig>();
    }
}
