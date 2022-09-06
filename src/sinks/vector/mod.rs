pub mod v2;

use vector_config::configurable_component;

use crate::config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext};

/// Marker type for the version two of the configuration for the `vector` sink.
#[configurable_component]
#[derive(Clone, Debug)]
enum VectorConfigVersion {
    /// Marker value for version two.
    #[serde(rename = "2")]
    V2,
}

/// Configurable for the `vector` sink.
#[configurable_component(sink("vector"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct VectorConfig {
    /// Version of the configuration.
    version: Option<VectorConfigVersion>,

    #[serde(flatten)]
    config: v2::VectorConfig,
}

impl GenerateConfig for VectorConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"version = "2"
            address = "127.0.0.1:6000"
            "#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
impl SinkConfig for VectorConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        self.config.build(cx).await
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.config.acknowledgements
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::VectorConfig>();
    }
}
