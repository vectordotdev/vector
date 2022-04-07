//! This pipelines transform is a bit complex and needs a simple example.
//!
//! If we consider the following example:
//!
//! ```toml
//! [transforms.my_pipelines]
//! type = "pipelines"
//! inputs = ["syslog"]
//!
//! [transforms.my_pipelines.logs]
//! order = ["foo", "bar"]
//!
//! [transforms.my_pipelines.logs.pipelines.foo]
//! name = "foo pipeline"
//!
//! [[transforms.my_pipelines.logs.pipelines.foo.transforms]]
//! # any transform configuration
//!
//! [[transforms.my_pipelines.logs.pipelines.foo.transforms]]
//! # any transform configuration
//!
//! [transforms.my_pipelines.logs.pipelines.bar]
//! name = "bar pipeline"
//!
//! [transforms.my_pipelines.logs.pipelines.bar.transforms]]
//! # any transform configuration
//!
//! [transforms.my_pipelines.metrics]
//! order = ["hello", "world"]
//!
//! [transforms.my_pipelines.metrics.pipelines.hello]
//! name = "hello pipeline"
//!
//! [[transforms.my_pipelines.metrics.pipelines.hello.transforms]]
//! # any transform configuration
//!
//! [[transforms.my_pipelines.metrics.pipelines.hello.transforms]]
//! # any transform configuration
//!
//! [transforms.my_pipelines.metrics.pipelines.world]
//! name = "world pipeline"
//!
//! [[transforms.my_pipelines.metrics.pipelines.world.transforms]]
//! # any transform configuration
//! ```
//!
//! The pipelines transform will first expand into 2 parallel transforms for `logs` and
//! `metrics`. A `Noop` transform will be also added to aggregate `logs` and `metrics`
//! into a single transform and to be able to use the transform name (`my_pipelines`) as an input.
//!
//! Then the `logs` group of pipelines will be expanded into a `EventFilter` followed by
//! a series `PipelineConfig` via the `EventRouter` transform. At the end, a `Noop` alias is added
//! to be able to refer `logs` as `my_pipelines.logs`.
//! Same thing for the `metrics` group of pipelines.
//!
//! Each pipeline will then be expanded into a list of its transforms and at the end of each
//! expansion, a `Noop` transform will be added to use the `pipeline` name as an alias
//! (`my_pipelines.logs.transforms.foo`).
mod config;
// mod expander;
// mod filter;
// mod router;

use crate::{
    conditions::is_log::IsLogConfig,
    conditions::is_metric::IsMetricConfig,
    conditions::AnyCondition,
    config::{GenerateConfig, TransformDescription},
    schema,
    transforms::route::{RouteConfig, UNMATCHED_ROUTE},
};
use config::EventTypeConfig;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt::Debug};
use vector_core::{
    config::{ComponentKey, DataType, Input, Output},
    transform::{
        InnerTopology, InnerTopologyTransform, Transform, TransformConfig, TransformContext,
    },
};

//------------------------------------------------------------------------------

inventory::submit! {
    TransformDescription::new::<PipelinesConfig>("pipelines")
}

/// The configuration of the pipelines transform itself.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct PipelinesConfig {
    #[serde(default)]
    logs: EventTypeConfig,
    #[serde(default)]
    metrics: EventTypeConfig,
}

#[cfg(test)]
impl PipelinesConfig {
    #[allow(dead_code)] // for some small subset of feature flags this code is dead
    pub(crate) const fn logs(&self) -> &EventTypeConfig {
        &self.logs
    }

    #[allow(dead_code)] // for some small subset of feature flags this code is dead
    pub(crate) const fn metrics(&self) -> &EventTypeConfig {
        &self.metrics
    }
}

impl PipelinesConfig {
    fn validate_nesting(&self) -> crate::Result<()> {
        let parents = &[self.transform_type()].into_iter().collect::<HashSet<_>>();
        self.logs.validate_nesting(parents)?;
        self.metrics.validate_nesting(parents)?;
        Ok(())
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
        name: &ComponentKey,
        inputs: &[String],
    ) -> crate::Result<Option<InnerTopology>> {
        self.validate_nesting()?;
        let router_name = name.join("type_router");
        let mut result = InnerTopology {
            inner: Default::default(),
            // the default route of the type router should always be redirected
            outputs: vec![(
                router_name.clone(),
                vec![Output::from((UNMATCHED_ROUTE, DataType::all()))],
            )],
        };
        let mut conditions = IndexMap::new();
        if !self.logs.is_empty() {
            let logs_route = name.join("logs");
            conditions.insert(
                "logs".to_string(),
                AnyCondition::Map(Box::new(IsLogConfig {})),
            );
            let logs_inputs = vec![router_name.port("logs")];
            let inner_topology = self
                .logs
                .expand(&logs_route, &logs_inputs)?
                .ok_or("Unable to expand pipeline stream")?;
            result.inner.extend(inner_topology.inner.into_iter());
            result.outputs.extend(inner_topology.outputs.into_iter());
        }
        if !self.metrics.is_empty() {
            let metrics_route = name.join("metrics");
            conditions.insert(
                "metrics".to_string(),
                AnyCondition::Map(Box::new(IsMetricConfig {})),
            );
            let metrics_inputs = vec![router_name.port("metrics")];
            let inner_topology = self
                .metrics
                .expand(&metrics_route, &metrics_inputs)?
                .ok_or("Unable to expand pipeline stream")?;
            result.inner.extend(inner_topology.inner.into_iter());
            result.outputs.extend(inner_topology.outputs.into_iter());
        }
        result.inner.insert(
            router_name,
            InnerTopologyTransform {
                inputs: inputs.to_vec(),
                inner: Box::new(RouteConfig::new(conditions)),
            },
        );
        Ok(Some(result))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn outputs(&self, _: &schema::Definition) -> Vec<Output> {
        vec![Output::default(DataType::all())]
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
        assert_eq!(config.logs.pipelines().len(), 2);
        let foo = config.logs.pipelines().get("foo").unwrap();
        assert_eq!(foo.transforms().len(), 2);
        let bar = config.logs.pipelines().get("bar").unwrap();
        assert_eq!(bar.transforms().len(), 1);
    }

    #[test]
    fn expanding() {
        let config = PipelinesConfig::generate_config();
        let config: PipelinesConfig = config.try_into().unwrap();
        let outer = TransformOuter {
            inputs: vec!["source".to_string()],
            inner: Box::new(config),
        };
        let name = ComponentKey::from("foo");
        let mut transforms = IndexMap::new();
        let mut expansions = IndexMap::new();
        let parents = HashSet::new();
        outer
            .expand(name, &parents, &mut transforms, &mut expansions)
            .unwrap();
        let routes = transforms
            .iter()
            .map(|(key, transform)| (key.to_string(), transform.inputs.clone()))
            .collect::<IndexMap<String, Vec<String>>>();
        let expansions: IndexMap<String, Vec<String>> = expansions
            .into_iter()
            .map(|(key, others)| {
                (
                    key.to_string(),
                    others.iter().map(ToString::to_string).collect(),
                )
            })
            .collect();
        assert_eq!(
            transforms
                .keys()
                .map(|key| key.to_string())
                .collect::<Vec<String>>(),
            vec![
                "foo.logs.foo.filter",
                "foo.logs.foo.0",
                "foo.logs.foo.1",
                "foo.logs.bar.0",
                "foo.type_router",
            ],
        );
        assert_eq!(routes["foo.type_router"], vec!["source".to_string()]);
        assert_eq!(
            routes["foo.logs.foo.filter"],
            vec!["foo.type_router.logs".to_string()]
        );
        assert_eq!(
            routes["foo.logs.foo.0"],
            vec!["foo.logs.foo.filter.success".to_string()]
        );
        assert_eq!(routes["foo.logs.foo.1"], vec!["foo.logs.foo.0".to_string()]);
        assert_eq!(
            routes["foo.logs.bar.0"],
            vec![
                "foo.logs.foo.1".to_string(),
                "foo.logs.foo.filter._unmatched".to_string(),
            ],
        );
        assert_eq!(
            expansions["foo"],
            vec!["foo.type_router._unmatched", "foo.logs.bar.0"]
        );
    }
}
