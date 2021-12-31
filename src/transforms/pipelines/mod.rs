/// This pipelines transform is a bit complex and needs a simple example.
///
/// If we consider the following example:
///
/// ```toml
/// [transforms.my_pipelines]
/// type = "pipelines"
/// inputs = ["syslog"]
///
/// [transforms.my_pipelines.logs]
/// order = ["foo", "bar"]
///
/// [transforms.my_pipelines.logs.pipelines.foo]
/// name = "foo pipeline"
///
/// [[transforms.my_pipelines.logs.pipelines.foo.transforms]]
/// # any transform configuration
///
/// [[transforms.my_pipelines.logs.pipelines.foo.transforms]]
/// # any transform configuration
///
/// [transforms.my_pipelines.logs.pipelines.bar]
/// name = "bar pipeline"
///
/// [transforms.my_pipelines.logs.pipelines.bar.transforms]]
/// # any transform configuration
///
/// [transforms.my_pipelines.metrics]
/// order = ["hello", "world"]
///
/// [transforms.my_pipelines.metrics.pipelines.hello]
/// name = "hello pipeline"
///
/// [[transforms.my_pipelines.metrics.pipelines.hello.transforms]]
/// # any transform configuration
///
/// [[transforms.my_pipelines.metrics.pipelines.hello.transforms]]
/// # any transform configuration
///
/// [transforms.my_pipelines.metrics.pipelines.world]
/// name = "world pipeline"
///
/// [[transforms.my_pipelines.metrics.pipelines.world.transforms]]
/// # any transform configuration
/// ```
///
/// The pipelines transform will first expand into 2 parallel transforms for `logs` and
/// `metrics`. A `Noop` transform will be also added to aggregate `logs` and `metrics`
/// into a single transform and to be able to use the transform name (`my_pipelines`) as an input.
///
/// Then the `logs` group of pipelines will be expanded into a `EventFilter` followed by
/// a series `PipelineConfig` via the `EventRouter` transform. At the end, a `Noop` alias is added
/// to be able to refer `logs` as `my_pipelines.logs`.
/// Same thing for the `metrics` group of pipelines.
///
/// Each pipeline will then be expanded into a list of its transforms and at the end of each
/// expansion, a `Noop` transform will be added to use the `pipeline` name as an alias
/// (`my_pipelines.logs.transforms.foo`).
mod expander;
mod filter;
mod router;

use std::collections::HashSet;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    conditions::AnyCondition,
    config::{
        DataType, ExpandType, GenerateConfig, TransformConfig, TransformContext,
        TransformDescription,
    },
    transforms::Transform,
};

inventory::submit! {
    TransformDescription::new::<PipelinesConfig>("pipelines")
}

/// This represents the configuration of a single pipeline, not the pipelines transform
/// itself, which can contain multiple individual pipelines
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct PipelineConfig {
    name: String,
    filter: Option<AnyCondition>,
    transforms: Vec<Box<dyn TransformConfig>>,
}

#[cfg(test)]
impl PipelineConfig {
    pub fn transforms(&self) -> &Vec<Box<dyn TransformConfig>> {
        &self.transforms
    }
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
    /// Expands a single pipeline into a series of its transforms.
    fn serial(&self) -> Box<dyn TransformConfig> {
        let transforms: IndexMap<String, Box<dyn TransformConfig>> = self
            .transforms
            .iter()
            .enumerate()
            .map(|(index, config)| (index.to_string(), config.clone()))
            .collect();
        let transforms = Box::new(expander::ExpanderConfig::serial(transforms));
        if let Some(ref filter) = self.filter {
            Box::new(filter::PipelineFilterConfig::new(
                filter.clone(),
                transforms,
            ))
        } else {
            transforms
        }
    }
}

/// This represent an ordered list of pipelines depending on the event type.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventTypeConfig {
    #[serde(default)]
    order: Option<Vec<String>>,
    pipelines: IndexMap<String, PipelineConfig>,
}

#[cfg(test)]
impl EventTypeConfig {
    pub const fn order(&self) -> &Option<Vec<String>> {
        &self.order
    }

    pub const fn pipelines(&self) -> &IndexMap<String, PipelineConfig> {
        &self.pipelines
    }
}

impl EventTypeConfig {
    fn is_empty(&self) -> bool {
        self.pipelines.is_empty()
    }

    fn names(&self) -> Vec<String> {
        if let Some(ref names) = self.order {
            // This assumes all the pipelines are present in the `order` field.
            // If a pipeline is missing, it won't be used.
            names.clone()
        } else {
            let mut names = self.pipelines.keys().cloned().collect::<Vec<String>>();
            names.sort();
            names
        }
    }

    /// Expands a group of pipelines into a series of pipelines.
    /// They will then be expanded into a series of transforms.
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

/// The configuration of the pipelines transform itself.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PipelinesConfig {
    #[serde(default)]
    logs: EventTypeConfig,
    #[serde(default)]
    metrics: EventTypeConfig,
}

#[cfg(test)]
impl PipelinesConfig {
    pub const fn logs(&self) -> &EventTypeConfig {
        &self.logs
    }

    pub const fn metrics(&self) -> &EventTypeConfig {
        &self.metrics
    }
}

impl PipelinesConfig {
    /// Transforms the actual transform in 2 parallel transforms.
    /// They are wrapped into an EventRouterConfig transform in order to filter logs and metrics.
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
            // need to aggregate to be able to use the transform name as an input.
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

    /// The pipelines transform shouldn't be embedded in another pipelines transform.
    fn nestable(&self, parents: &HashSet<&'static str>) -> bool {
        !parents.contains(&self.transform_type())
    }
}

impl GenerateConfig for PipelinesConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc::indoc! {r#"
            [logs]
            order = ["foo", "bar"]

            [logs.pipelines.foo]
            name = "foo pipeline"

            [logs.pipelines.foo.filter]
            type = "datadog_search"
            source = "source:s3"

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
        crate::config::format::deserialize(input, crate::config::format::Format::Toml).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use indexmap::IndexMap;

    use super::{GenerateConfig, PipelinesConfig};
    use crate::config::{ComponentKey, TransformOuter};

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

    #[test]
    fn expanding() {
        let config = PipelinesConfig::generate_config();
        let config: PipelinesConfig = config.try_into().unwrap();
        let outer = TransformOuter {
            inputs: Vec::<String>::new(),
            inner: Box::new(config),
        };
        let name = ComponentKey::from("foo");
        let mut transforms = IndexMap::new();
        let mut expansions = IndexMap::new();
        let parents = HashSet::new();
        outer
            .expand(name, &parents, &mut transforms, &mut expansions)
            .unwrap();
        assert_eq!(
            transforms
                .keys()
                .map(|key| key.to_string())
                .collect::<Vec<String>>(),
            vec![
                "foo.logs.filter",
                "foo.logs.pipelines.foo.truthy.filter",
                "foo.logs.pipelines.foo.truthy.transforms.0",
                "foo.logs.pipelines.foo.truthy.transforms.1",
                "foo.logs.pipelines.foo.truthy.transforms",
                "foo.logs.pipelines.foo.truthy",
                "foo.logs.pipelines.foo.falsy",
                "foo.logs.pipelines.foo",
                "foo.logs.pipelines.bar.0",
                "foo.logs.pipelines.bar",
                "foo.logs.pipelines",
                "foo.logs",
                "foo"
            ],
        );
    }
}
