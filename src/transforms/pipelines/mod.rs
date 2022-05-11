//! This pipelines transform is a bit complex and needs a simple example.
//!
//! If we consider the following example:
//!
//! ```toml
//! [transforms.my_pipelines]
//! type = "pipelines"
//! inputs = ["syslog"]
//!
//! [[transforms.my_pipelines.logs]]
//! name = "foo pipeline"
//!
//! [[transforms.my_pipelines.logs.transforms]]
//! # any transform configuration
//!
//! [[transforms.my_pipelines.logs.transforms]]
//! # any transform configuration
//!
//! [[transforms.my_pipelines.logs]]
//! name = "bar pipeline"
//! filter.type = "vrl"
//! filter.source = """some condition here"""
//!
//! [[transforms.my_pipelines.logs.transforms]]
//! # any transform configuration
//!
//! [[transforms.my_pipelines.metrics]]
//! name = "hello pipeline"
//! filter.type = "vrl"
//! filter.source = """some condition here"""
//!
//! [[transforms.my_pipelines.metrics.transforms]]
//! # any transform configuration
//!
//! [[transforms.my_pipelines.metrics.transforms]]
//! # any transform configuration
//!
//! [[transforms.my_pipelines.metrics]]
//! name = "world pipeline"
//!
//! [[transforms.my_pipelines.metrics.transforms]]
//! # any transform configuration
//!
//! [sinks.output]
//! inputs = ["my_pipelines"]
//! # any sink configuration
//! ```
//!
//! The pipelines transform will expand individually each pipeline and adjust all the inputs accordingly.
//! Once transpiled, the topology will have the same shape than the following configuration.
//!
//! ```toml
//! [transforms.my_pipelines_type_router]
//! inputs = ["syslog"]
//!
//! [transforms.my_pipelines_logs_0_transform_0]
//! inputs = ["my_pipelines_type_router.logs"]
//! # any transform configuration
//!
//! [transforms.my_pipelines_logs_0_transform_1]
//! inputs = ["my_pipelines_logs_0_transform_0"]
//! # any transform configuration
//!
//! [transforms.my_pipelines_logs_1_filter]
//! inputs = ["my_pipelines_logs_0_transform_1"]
//! type = "filter"
//! condition.type = "vrl"
//! condition.source = """some condition here"""
//!
//! [transforms.my_pipelines_logs_1_transform_0]
//! inputs = ["my_pipelines_logs_1_filter.success"]
//! # any transform configuration
//!
//! [transforms.my_pipelines_metrics_0_filter]
//! inputs = ["my_pipelines_type_router.metrics"]
//! type = "filter"
//! condition.type = "vrl"
//! condition.source = """some condition here"""
//!
//! [transforms.my_pipelines_metrics_0_transform_0]
//! inputs = ["my_pipelines_metrics_0_filter.success"]
//! # any transform configuration
//!
//! [transforms.my_pipelines_metrics_0_transform_1]
//! inputs = ["my_pipelines_metrics_0_transform_0"]
//! # any transform configuration
//!
//! [transforms.my_pipelines_metrics_1_transform_0]
//! inputs = [
//!     # the events filtered from the previous transform are forwarded
//!     "my_pipelines_metrics_0_filter._dropped",
//!     "my_pipelines_metrics_0_transform_1",
//! ]
//! # any transform configuration
//!
//! [sinks.output]
//! inputs = [
//!     "my_pipelines_type_router._dropped",
//!     # the events from the last logs pipeline are forwarded here
//!     "my_pipelines_logs_1_filter._dropped",
//!     "my_pipelines_logs_1_transform_0",
//!     "my_pipelines_metrics_1_transform_0",
//! ]
//! # any sink configuration
//! ```
mod config;

use std::{collections::HashSet, fmt::Debug};

use config::EventTypeConfig;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use vector_core::{
    config::{ComponentKey, DataType, Input, Output},
    transform::{
        InnerTopology, InnerTopologyTransform, Transform, TransformConfig, TransformContext,
    },
};

use crate::{
    conditions::is_log::IsLogConfig,
    conditions::is_metric::IsMetricConfig,
    conditions::AnyCondition,
    config::{GenerateConfig, TransformDescription},
    schema,
    transforms::route::{RouteConfig, UNMATCHED_ROUTE},
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
                vec![Output::default(DataType::all()).with_port(UNMATCHED_ROUTE)],
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
            [[logs]]
            name = "foo pipeline"

            [logs.filter]
            type = "datadog_search"
            source = "source:s3"

            [[logs.transforms]]
            type = "filter"
            condition = ""

            [[logs.transforms]]
            type = "filter"
            condition = ""

            [[logs]]
            name = "bar pipeline"

            [[logs.transforms]]
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
        assert_eq!(config.logs.as_ref().len(), 2);
        let foo = config.logs.as_ref().first().unwrap();
        assert_eq!(foo.transforms().len(), 2);
        let bar = config.logs.as_ref().get(1).unwrap();
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
                "foo.logs.0.filter",
                "foo.logs.0.0",
                "foo.logs.0.1",
                "foo.logs.1.0",
                "foo.type_router",
            ],
        );
        assert_eq!(routes["foo.type_router"], vec!["source".to_string()]);
        assert_eq!(
            routes["foo.logs.0.filter"],
            vec!["foo.type_router.logs".to_string()]
        );
        assert_eq!(
            routes["foo.logs.0.0"],
            vec!["foo.logs.0.filter.success".to_string()]
        );
        assert_eq!(routes["foo.logs.0.1"], vec!["foo.logs.0.0".to_string()]);
        assert_eq!(
            routes["foo.logs.1.0"],
            vec![
                "foo.logs.0.1".to_string(),
                "foo.logs.0.filter._unmatched".to_string(),
            ],
        );
        assert_eq!(
            expansions["foo"],
            vec!["foo.type_router._unmatched", "foo.logs.1.0"]
        );
    }
}
