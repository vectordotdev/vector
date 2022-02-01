pub mod v1;
pub mod v2;

use serde::{Deserialize, Serialize};

use crate::config::{
    GenerateConfig, Output, Resource, SourceConfig, SourceContext, SourceDescription,
};

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
}

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::VectorConfig>();
    }
}
