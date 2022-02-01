pub mod v1;
pub mod v2;

use serde::{Deserialize, Serialize};

use crate::config::{DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription};

#[derive(Serialize, Deserialize, Debug, Clone)]
enum V1 {
    #[serde(rename = "1")]
    V1,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct VectorConfigV1 {
    version: V1,
    #[serde(flatten)]
    config: v1::VectorConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
enum V2 {
    #[serde(rename = "2")]
    V2,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct VectorConfigV2 {
    version: Option<V2>,
    #[serde(flatten)]
    config: v2::VectorConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum VectorConfig {
    V1(VectorConfigV1),
    V2(VectorConfigV2),
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
            VectorConfig::V1(v1) => v1.config.build(cx).await,
            VectorConfig::V2(v2) => v2.config.build(cx).await,
        }
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn sink_type(&self) -> &'static str {
        "vector"
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::VectorConfig>();
    }
}
