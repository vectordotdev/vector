pub mod v1;
pub mod v2;

use vector_config::configurable_component;

use crate::config::{
    AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, SinkDescription,
};

/// Marker type for the version one of the configuration for the `vector` sink.
#[configurable_component]
#[derive(Clone, Debug)]
enum V1 {
    /// Marker value for version one.
    #[serde(rename = "1")]
    V1,
}

/// Configuration for version one of the `vector` sink.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct VectorConfigV1 {
    /// Version of the configuration.
    version: V1,

    #[serde(flatten)]
    config: v1::VectorConfig,
}

/// Marker type for the version two of the configuration for the `vector` sink.
#[configurable_component]
#[derive(Clone, Debug)]
enum V2 {
    /// Marker value for version two.
    #[serde(rename = "2")]
    V2,
}

/// Configuration for version two of the `vector` sink.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct VectorConfigV2 {
    /// Version of the configuration.
    version: Option<V2>,

    #[serde(flatten)]
    config: v2::VectorConfig,
}

/// Configurable for the `vector` sink.
#[configurable_component(sink)]
#[derive(Clone, Debug)]
#[serde(untagged)]
pub enum VectorConfig {
    /// Configuration for version one.
    V1(#[configurable(derived)] VectorConfigV1),

    /// Configuration for version two.
    V2(#[configurable(derived)] VectorConfigV2),
}

inventory::submit! {
    SinkDescription::new::<VectorConfig>("vector")
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
#[typetag::serde(name = "vector")]
impl SinkConfig for VectorConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        match self {
            VectorConfig::V1(v1) => v1.config.build().await,
            VectorConfig::V2(v2) => v2.config.build(cx).await,
        }
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn sink_type(&self) -> &'static str {
        "vector"
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        match self {
            Self::V1(v1) => &v1.config.acknowledgements,
            Self::V2(v2) => &v2.config.acknowledgements,
        }
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::VectorConfig>();
    }
}
