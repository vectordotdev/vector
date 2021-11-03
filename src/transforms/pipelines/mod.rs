mod expander;
mod router;

use crate::config::{
    DataType, ExpandType, GenerateConfig, TransformConfig, TransformContext, TransformDescription,
};
use crate::transforms::Transform;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

inventory::submit! {
    TransformDescription::new::<PipelinesConfig>("pipelines")
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct PipelineConfig {
    name: String,
    transforms: Vec<Box<dyn TransformConfig>>,
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

impl PipelineConfig {
    fn serial(&self) -> Box<dyn TransformConfig> {
        let pipelines: IndexMap<String, Box<dyn TransformConfig>> = self
            .transforms
            .iter()
            .enumerate()
            .map(|(index, config)| (index.to_string(), config.clone()))
            .collect();

        Box::new(expander::ExpanderConfig::serial(pipelines))
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventTypeConfig {
    #[serde(default)]
    order: Option<Vec<String>>,
    pipelines: IndexMap<String, PipelineConfig>,
}

impl EventTypeConfig {
    fn is_empty(&self) -> bool {
        self.pipelines.is_empty()
    }

    fn names(&self) -> Vec<String> {
        if let Some(ref names) = self.order {
            names.clone()
        } else {
            let mut names = self.pipelines.keys().cloned().collect::<Vec<String>>();
            names.sort();
            names
        }
    }

    fn serial(&self) -> Box<dyn TransformConfig> {
        let pipelines: IndexMap<String, Box<dyn TransformConfig>> = self
            .names()
            .into_iter()
            .filter_map(|name| {
                self.pipelines
                    .get(&name)
                    .map(|config| (name, config.serial()))
            })
            .collect();

        Box::new(expander::ExpanderConfig::serial(pipelines))
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PipelinesConfig {
    #[serde(default)]
    logs: EventTypeConfig,
    #[serde(default)]
    metrics: EventTypeConfig,
}

impl PipelinesConfig {
    fn parallel(&self) -> IndexMap<String, Box<dyn TransformConfig>> {
        let mut map: IndexMap<String, Box<dyn TransformConfig>> = IndexMap::new();

        if !self.logs.is_empty() {
            map.insert(
                "logs".to_string(),
                Box::new(router::EventRouterConfig::log(self.logs.serial())),
            );
        }

        if !self.metrics.is_empty() {
            map.insert(
                "metrics".to_string(),
                Box::new(router::EventRouterConfig::metric(self.metrics.serial())),
            );
        }

        map
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "pipelines")]
impl TransformConfig for PipelinesConfig {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        Err("this transform must be expanded".into())
    }

    fn expand(
        &mut self,
    ) -> crate::Result<Option<(IndexMap<String, Box<dyn TransformConfig>>, ExpandType)>> {
        Ok(Some((
            self.parallel(),
            ExpandType::Parallel { aggregates: true },
        )))
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
        toml::from_str(indoc::indoc! {r#"
            [logs]
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
            condition = ""
        "#})
        .unwrap()
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
    use super::{GenerateConfig, PipelinesConfig};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<PipelinesConfig>();
    }

    #[test]
    fn parsing() {
        let config = PipelinesConfig::generate_config();
        let config: PipelinesConfig = config.try_into().unwrap();
        assert_eq!(config.logs.pipelines.len(), 2);
        let foo = config.logs.pipelines.get("foo").unwrap();
        assert_eq!(foo.transforms.len(), 2);
        let bar = config.logs.pipelines.get("bar").unwrap();
        assert_eq!(bar.transforms.len(), 1);
    }
}
