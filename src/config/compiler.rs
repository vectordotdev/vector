use std::collections::HashSet;

use indexmap::{IndexMap, IndexSet};

use super::{builder::ConfigBuilder, graph::Graph, validation, ComponentKey, Config, OutputId};

pub fn compile(mut builder: ConfigBuilder) -> Result<(Config, Vec<String>), Vec<String>> {
    let mut errors = Vec::new();

    // component names should not have dots in the configuration file
    // but components can expand (like route) to have components with a dot
    // so this check should be done before expanding components
    if let Err(name_errors) = validation::check_names(
        builder
            .transforms
            .keys()
            .chain(builder.sources.keys())
            .chain(builder.sinks.keys()),
    ) {
        errors.extend(name_errors);
    }

    let expansions = expand_macros(&mut builder)?;

    expand_globs(&mut builder);

    if let Err(type_errors) = validation::check_shape(&builder) {
        errors.extend(type_errors);
    }

    if let Err(type_errors) = validation::check_resources(&builder) {
        errors.extend(type_errors);
    }

    if let Err(output_errors) = validation::check_outputs(&builder) {
        errors.extend(output_errors);
    }

    #[cfg(feature = "datadog-pipelines")]
    let version = Some(builder.sha256_hash());

    #[cfg(not(feature = "datadog-pipelines"))]
    let version = None;

    let ConfigBuilder {
        global,
        #[cfg(feature = "api")]
        api,
        #[cfg(feature = "datadog-pipelines")]
        datadog,
        healthchecks,
        enrichment_tables,
        sources,
        sinks,
        transforms,
        tests,
        provider: _,
    } = builder;

    let graph = match Graph::new(&sources, &transforms, &sinks) {
        Ok(graph) => graph,
        Err(graph_errors) => {
            errors.extend(graph_errors);
            return Err(errors);
        }
    };

    if let Err(type_errors) = graph.typecheck() {
        errors.extend(type_errors);
    }

    if let Err(e) = graph.check_for_cycles() {
        errors.push(e);
    }

    // Inputs are resolved from string into OutputIds as part of graph construction, so update them
    // here before adding to the final config (the types require this).
    let sinks = sinks
        .into_iter()
        .map(|(key, sink)| {
            let inputs = graph.inputs_for(&key);
            (key, sink.with_inputs(inputs))
        })
        .collect();
    let transforms = transforms
        .into_iter()
        .map(|(key, transform)| {
            let inputs = graph.inputs_for(&key);
            (key, transform.with_inputs(inputs))
        })
        .collect();
    let tests = tests
        .into_iter()
        .map(|test| test.resolve_outputs(&graph))
        .collect();

    if errors.is_empty() {
        let config = Config {
            global,
            #[cfg(feature = "api")]
            api,
            #[cfg(feature = "datadog-pipelines")]
            datadog,
            version,
            healthchecks,
            enrichment_tables,
            sources,
            sinks,
            transforms,
            tests,
            expansions,
        };

        let warnings = validation::warnings(&config);

        Ok((config, warnings))
    } else {
        Err(errors)
    }
}

/// Some component configs can act like macros and expand themselves into multiple replacement
/// configs. Performs those expansions and records the relevant metadata.
pub(super) fn expand_macros(
    config: &mut ConfigBuilder,
) -> Result<IndexMap<ComponentKey, Vec<ComponentKey>>, Vec<String>> {
    let mut expanded_transforms = IndexMap::new();
    let mut expansions = IndexMap::new();
    let mut errors = Vec::new();
    let parent_types = HashSet::new();

    while let Some((key, transform)) = config.transforms.pop() {
        if let Err(error) = transform.expand(
            key,
            &parent_types,
            &mut expanded_transforms,
            &mut expansions,
        ) {
            errors.push(error);
        }
    }
    config.transforms = expanded_transforms;

    if !errors.is_empty() {
        Err(errors)
    } else {
        Ok(expansions)
    }
}

/// Expand globs in input lists
pub fn expand_globs(config: &mut ConfigBuilder) {
    let candidates = config
        .sources
        .iter()
        .flat_map(|(key, s)| {
            s.inner.outputs().into_iter().map(|output| OutputId {
                component: key.clone(),
                port: output.port,
            })
        })
        .chain(config.transforms.iter().flat_map(|(key, t)| {
            t.inner.outputs().into_iter().map(|output| OutputId {
                component: key.clone(),
                port: output.port,
            })
        }))
        .map(|output_id| output_id.to_string())
        .collect::<IndexSet<String>>();

    for (id, transform) in config.transforms.iter_mut() {
        expand_globs_inner(&mut transform.inputs, &id.to_string(), &candidates);
    }

    for (id, sink) in config.sinks.iter_mut() {
        expand_globs_inner(&mut sink.inputs, &id.to_string(), &candidates);
    }
}

enum InputMatcher {
    Pattern(glob::Pattern),
    String(String),
}

impl InputMatcher {
    fn matches(&self, candidate: &str) -> bool {
        use InputMatcher::*;

        match self {
            Pattern(pattern) => pattern.matches(candidate),
            String(s) => s == candidate,
        }
    }
}

fn expand_globs_inner(inputs: &mut Vec<String>, id: &str, candidates: &IndexSet<String>) {
    let raw_inputs = std::mem::take(inputs);
    for raw_input in raw_inputs {
        let matcher = glob::Pattern::new(&raw_input)
            .map(InputMatcher::Pattern)
            .unwrap_or_else(|error| {
                warn!(message = "Invalid glob pattern for input.", component_id = %id, %error);
                InputMatcher::String(raw_input.to_string())
            });
        let mut matched = false;
        for input in candidates {
            if matcher.matches(input) && input != id {
                matched = true;
                inputs.push(input.clone())
            }
        }
        // If it didn't work as a glob pattern, leave it in the inputs as-is. This lets us give
        // more accurate error messages about non-existent inputs.
        if !matched {
            inputs.push(raw_input)
        }
    }
}

#[cfg(test)]
mod test {
    use async_trait::async_trait;
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::{
        config::{
            DataType, Output, SinkConfig, SinkContext, SourceConfig, SourceContext,
            TransformConfig, TransformContext,
        },
        sinks::{Healthcheck, VectorSink},
        sources::Source,
        transforms::Transform,
    };

    #[derive(Debug, Serialize, Deserialize)]
    struct MockSourceConfig;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct MockTransformConfig;

    #[derive(Debug, Serialize, Deserialize)]
    struct MockSinkConfig;

    #[async_trait]
    #[typetag::serde(name = "mock")]
    impl SourceConfig for MockSourceConfig {
        async fn build(&self, _cx: SourceContext) -> crate::Result<Source> {
            unimplemented!()
        }

        fn source_type(&self) -> &'static str {
            "mock"
        }

        fn outputs(&self) -> Vec<Output> {
            vec![Output::default(DataType::Any)]
        }
    }

    #[async_trait]
    #[typetag::serde(name = "mock")]
    impl TransformConfig for MockTransformConfig {
        async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
            unimplemented!()
        }

        fn transform_type(&self) -> &'static str {
            "mock"
        }

        fn input_type(&self) -> DataType {
            DataType::Any
        }

        fn outputs(&self) -> Vec<Output> {
            vec![Output::default(DataType::Any)]
        }
    }

    #[async_trait]
    #[typetag::serde(name = "mock")]
    impl SinkConfig for MockSinkConfig {
        async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
            unimplemented!()
        }

        fn sink_type(&self) -> &'static str {
            "mock"
        }

        fn input_type(&self) -> DataType {
            DataType::Any
        }
    }

    #[test]
    fn glob_expansion() {
        let mut builder = ConfigBuilder::default();
        builder.add_source("foo1", MockSourceConfig);
        builder.add_source("foo2", MockSourceConfig);
        builder.add_source("bar", MockSourceConfig);
        builder.add_transform("foos", &["foo*"], MockTransformConfig);
        builder.add_sink("baz", &["foos*", "b*"], MockSinkConfig);
        builder.add_sink("quix", &["*oo*"], MockSinkConfig);
        builder.add_sink("quux", &["*"], MockSinkConfig);

        let config = builder.build().expect("build should succeed");

        assert_eq!(
            config
                .transforms
                .get(&ComponentKey::from("foos"))
                .map(|item| without_ports(item.inputs.clone()))
                .unwrap(),
            vec![ComponentKey::from("foo1"), ComponentKey::from("foo2")]
        );
        assert_eq!(
            config
                .sinks
                .get(&ComponentKey::from("baz"))
                .map(|item| without_ports(item.inputs.clone()))
                .unwrap(),
            vec![ComponentKey::from("foos"), ComponentKey::from("bar")]
        );
        assert_eq!(
            config
                .sinks
                .get(&ComponentKey::from("quux"))
                .map(|item| without_ports(item.inputs.clone()))
                .unwrap(),
            vec![
                ComponentKey::from("foo1"),
                ComponentKey::from("foo2"),
                ComponentKey::from("bar"),
                ComponentKey::from("foos")
            ]
        );
        assert_eq!(
            config
                .sinks
                .get(&ComponentKey::from("quix"))
                .map(|item| without_ports(item.inputs.clone()))
                .unwrap(),
            vec![
                ComponentKey::from("foo1"),
                ComponentKey::from("foo2"),
                ComponentKey::from("foos")
            ]
        );
    }

    fn without_ports(outputs: Vec<OutputId>) -> Vec<ComponentKey> {
        outputs
            .into_iter()
            .map(|output| {
                assert!(output.port.is_none());
                output.component
            })
            .collect()
    }
}
