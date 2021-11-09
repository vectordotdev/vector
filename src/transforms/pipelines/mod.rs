/// This pipelines transform is a bit complex and needs a simple example.
///
/// If we take the following example in consideration
///
/// ```toml
/// [transforms.my_pipelines]
/// type = "pipelines"
/// inputs = ["syslog"]
/// mode = "serial"
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
///
/// If we change the `mode` to `parallel`, then most of the aliases will be dropped so that you can
/// target each pipeline as an input. In our example, if we switch to `parallel` mode, it won't be
/// possible to target `my_pipelines` as an input. Instead, `my_pipelines.logs.pipelines.foo`,
/// `my_pipelines.logs.pipelines.bar`, `my_pipelines.metrics.pipelines.hello` and
/// `my_pipelines.metrics.world` will be exposed to be used as an input.
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

/// This represents the configuration of a single pipeline,
/// not the pipelines transform itself.
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
    /// Expands a single pipeline into a series of its transforms.
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

/// This represent an ordered list of pipelines depending on the event type.
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
    /// If the serial mode is used, they will then be expanded into a series of transforms and an
    /// aggregating transform will be added.
    /// If the parallel mode is used, they will then be expanded in parallel and no aggregating
    /// transform will be added.
    fn expand(&self, mode: &PipelineMode) -> Box<dyn TransformConfig> {
        let pipelines: IndexMap<String, Box<dyn TransformConfig>> = self
            .names()
            .into_iter()
            .filter_map(|name| {
                self.pipelines
                    .get(&name)
                    .map(|config| (name, config.serial()))
            })
            .collect();

        match mode {
            PipelineMode::Serial => Box::new(expander::ExpanderConfig::serial(pipelines)),
            PipelineMode::Parallel => {
                Box::new(expander::ExpanderConfig::parallel(pipelines, false))
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PipelineMode {
    Parallel,
    Serial,
}

impl Default for PipelineMode {
    fn default() -> Self {
        Self::Serial
    }
}

impl PipelineMode {
    pub fn alias(&self) -> bool {
        matches!(self, Self::Serial)
    }
}

/// The configuration of the pipelines transform itself.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PipelinesConfig {
    #[serde(default)]
    mode: PipelineMode,
    #[serde(default)]
    logs: EventTypeConfig,
    #[serde(default)]
    metrics: EventTypeConfig,
}

impl PipelinesConfig {
    /// Transforms the actual transform in 2 parallel transforms.
    /// They are wrapped into an EventRouterConfig transform in order to filter logs and metrics.
    fn parallel(&self) -> IndexMap<String, Box<dyn TransformConfig>> {
        let mut map: IndexMap<String, Box<dyn TransformConfig>> = IndexMap::new();

        if !self.logs.is_empty() {
            map.insert(
                "logs".to_string(),
                Box::new(router::EventRouterConfig::log(
                    self.logs.expand(&self.mode),
                    self.mode.alias(),
                )),
            );
        }

        if !self.metrics.is_empty() {
            map.insert(
                "metrics".to_string(),
                Box::new(router::EventRouterConfig::metric(
                    self.metrics.expand(&self.mode),
                    self.mode.alias(),
                )),
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
            // when using serial mode, we need to make a single endpoint, so we need to
            // aggregate.
            ExpandType::Parallel {
                aggregates: self.mode.alias(),
            },
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
            mode = "serial"

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
    use super::{GenerateConfig, PipelineMode, PipelinesConfig};
    use crate::config::{ComponentKey, TransformOuter};
    use indexmap::IndexMap;

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
    fn expanding_serial() {
        let config = PipelinesConfig::generate_config();
        let config: PipelinesConfig = config.try_into().unwrap();
        let outer = TransformOuter {
            inputs: Vec::<String>::new(),
            inner: Box::new(config),
        };
        let name = ComponentKey::global("foo");
        let mut transforms = IndexMap::new();
        let mut expansions = IndexMap::new();
        outer
            .expand(name, &mut transforms, &mut expansions)
            .unwrap();
        assert_eq!(transforms.len(), 9);
        assert_eq!(
            transforms
                .keys()
                .map(|key| key.to_string())
                .collect::<Vec<String>>(),
            vec![
                "foo.logs.filter",
                "foo.logs.pipelines.foo.0",
                "foo.logs.pipelines.foo.1",
                "foo.logs.pipelines.foo",
                "foo.logs.pipelines.bar.0",
                "foo.logs.pipelines.bar",
                "foo.logs.pipelines",
                "foo.logs",
                "foo"
            ],
        );
        let foo_logs = transforms
            .get(&ComponentKey::global("foo.logs.pipelines"))
            .unwrap();
        assert_eq!(foo_logs.inputs.len(), 1);
    }

    #[test]
    fn expanding_parallel() {
        let config = PipelinesConfig::generate_config();
        let mut config: PipelinesConfig = config.try_into().unwrap();
        config.mode = PipelineMode::Parallel;
        let outer = TransformOuter {
            inputs: Vec::<String>::new(),
            inner: Box::new(config),
        };
        let name = ComponentKey::global("foo");
        let mut transforms = IndexMap::new();
        let mut expansions = IndexMap::new();
        outer
            .expand(name, &mut transforms, &mut expansions)
            .unwrap();
        // assert_eq!(transforms.len(), 9);
        assert_eq!(
            transforms
                .keys()
                .map(|key| key.to_string())
                .collect::<Vec<String>>(),
            vec![
                "foo.logs.filter",
                "foo.logs.pipelines.foo.0",
                "foo.logs.pipelines.foo.1",
                "foo.logs.pipelines.foo",
                "foo.logs.pipelines.bar.0",
                "foo.logs.pipelines.bar",
            ],
        );
    }
}
