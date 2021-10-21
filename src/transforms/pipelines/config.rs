use crate::config::{
    ComponentKey, DataType, GenerateConfig, TransformConfig, TransformContext,
    TransformDescription, TransformOuter,
};
use crate::transforms::Transform;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

const SAMPLE_CONFIG: &'static str = r#"[logs]
order = ["foo", "bar"]

[logs.pipelines.foo]
name = "foo pipeline"

[[logs.pipelines.foo.transforms]]
type = "filter"
condition = ""

[[logs.pipelines.foo.transforms]]
type = "filter"
condition = ""

[logs.pipelines.bar]
name = "bar pipeline"

[[logs.pipelines.bar.transforms]]
type = "filter"
condition = """#;

inventory::submit! {
    TransformDescription::new::<PipelinesConfig>("pipelines")
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineConfig {
    name: String,
    transforms: Vec<TransformOuter<String>>,
}

impl Clone for PipelineConfig {
    fn clone(&self) -> Self {
        // This is a hack around the issue of cloning
        // trait objects. So instead to clone the config
        // we first serialize it into JSON, then back from
        // JSON. Originally we used TOML here but TOML does not
        // support serializing `None`.
        let json = serde_json::to_value(self).unwrap();
        serde_json::from_value(json).unwrap()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventTypeConfig {
    #[serde(default)]
    order: Option<Vec<String>>,
    pipelines: IndexMap<ComponentKey, PipelineConfig>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PipelinesConfig {
    #[serde(default)]
    logs: EventTypeConfig,
    #[serde(default)]
    metrics: EventTypeConfig,
    #[serde(default)]
    traces: EventTypeConfig,
}

#[async_trait::async_trait]
#[typetag::serde(name = "pipelines")]
impl TransformConfig for PipelinesConfig {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        unimplemented!()
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn output_type(&self) -> DataType {
        DataType::Any
    }

    fn transform_type(&self) -> &'static str {
        "pipelines"
    }
}

impl GenerateConfig for PipelinesConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(SAMPLE_CONFIG).unwrap()
    }
}

#[cfg(test)]
impl PipelinesConfig {
    pub fn from_toml(input: &str) -> Self {
        crate::config::format::deserialize(input, Some(crate::config::format::Format::Toml))
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::{PipelinesConfig, SAMPLE_CONFIG};
    use crate::config::ComponentKey;

    #[test]
    fn parsing() {
        let config = PipelinesConfig::from_toml(SAMPLE_CONFIG);
        assert_eq!(config.logs.pipelines.len(), 2);
        let foo = config.logs.pipelines.get(&ComponentKey::from("foo")).unwrap();
        assert_eq!(foo.transforms.len(), 2);
        let bar = config.logs.pipelines.get(&ComponentKey::from("bar")).unwrap();
        assert_eq!(bar.transforms.len(), 1);
    }
}
