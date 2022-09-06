pub mod v2;

use vector_config::configurable_component;
use vector_core::config::LogNamespace;

use crate::config::{GenerateConfig, Output, Resource, SourceConfig, SourceContext};

/// Marker type for the version two of the configuration for the `vector` source.
#[configurable_component]
#[derive(Clone, Debug)]
enum VectorConfigVersion {
    /// Marker value for version two.
    #[serde(rename = "2")]
    V2,
}

/// Configurable for the `vector` source.
#[configurable_component(source("vector"))]
#[derive(Clone, Debug)]
pub struct VectorConfig {
    /// Version of the configuration.
    version: Option<VectorConfigVersion>,

    #[serde(flatten)]
    config: v2::VectorConfig,
}

impl GenerateConfig for VectorConfig {
    fn generate_config() -> toml::Value {
        let config =
            toml::Value::try_into::<v2::VectorConfig>(v2::VectorConfig::generate_config()).unwrap();
        toml::Value::try_from(VectorConfig {
            version: Some(VectorConfigVersion::V2),
            config,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
impl SourceConfig for VectorConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        self.config.build(cx).await
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<Output> {
        self.config.outputs()
    }

    fn resources(&self) -> Vec<Resource> {
        self.config.resources()
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
