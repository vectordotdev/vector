use super::{builder::ConfigBuilder, handle_warnings, validation, Config, TransformOuter};
use indexmap::IndexMap;

pub fn compile(mut builder: ConfigBuilder, deny_warnings: bool) -> Result<Config, Vec<String>> {
    let mut errors = Vec::new();

    expand_wildcards(&mut builder);

    let expansions = expand_macros(&mut builder)?;

    if let Err(warn) = handle_warnings(validation::warnings(&builder), deny_warnings) {
        errors.extend(warn);
    }

    if let Err(type_errors) = validation::check_shape(&builder) {
        errors.extend(type_errors);
    }

    if let Err(type_errors) = validation::typecheck(&builder) {
        errors.extend(type_errors);
    }

    if let Err(type_errors) = validation::check_resources(&builder) {
        errors.extend(type_errors);
    }

    if errors.is_empty() {
        Ok(Config {
            global: builder.global,
            #[cfg(feature = "api")]
            api: builder.api,
            healthchecks: builder.healthchecks,
            sources: builder.sources,
            sinks: builder.sinks,
            transforms: builder.transforms,
            tests: builder.tests,
            expansions,
        })
    } else {
        Err(errors)
    }
}

/// Some component configs can act like macros and expand themselves into multiple replacement
/// configs. Performs those expansions and records the relevant metadata.
pub(super) fn expand_macros(
    config: &mut ConfigBuilder,
) -> Result<IndexMap<String, Vec<String>>, Vec<String>> {
    let mut expanded_transforms = IndexMap::new();
    let mut expansions = IndexMap::new();
    let mut errors = Vec::new();

    while let Some((k, mut t)) = config.transforms.pop() {
        if let Some(expanded) = match t.inner.expand() {
            Ok(e) => e,
            Err(err) => {
                errors.push(format!("failed to expand transform '{}': {}", k, err));
                continue;
            }
        } {
            let mut children = Vec::new();
            for (name, child) in expanded {
                let full_name = format!("{}.{}", k, name);
                expanded_transforms.insert(
                    full_name.clone(),
                    TransformOuter {
                        inputs: t.inputs.clone(),
                        inner: child,
                    },
                );
                children.push(full_name);
            }
            expansions.insert(k.clone(), children);
        } else {
            expanded_transforms.insert(k, t);
        }
    }
    config.transforms = expanded_transforms;

    if !errors.is_empty() {
        Err(errors)
    } else {
        Ok(expansions)
    }
}

/// Expand trailing `*` wildcards in input lists
fn expand_wildcards(config: &mut ConfigBuilder) {
    let candidates = config
        .sources
        .keys()
        .chain(config.transforms.keys())
        .cloned()
        .collect::<Vec<String>>();

    for (name, transform) in config.transforms.iter_mut() {
        expand_wildcards_inner(&mut transform.inputs, name, &candidates);
    }

    for (name, sink) in config.sinks.iter_mut() {
        expand_wildcards_inner(&mut sink.inputs, name, &candidates);
    }
}

fn expand_wildcards_inner(inputs: &mut Vec<String>, name: &str, candidates: &[String]) {
    let raw_inputs = std::mem::take(inputs);
    for raw_input in raw_inputs {
        if raw_input.ends_with('*') {
            let prefix = &raw_input[0..raw_input.len() - 1];
            for input in candidates {
                if input.starts_with(prefix) && input != name {
                    inputs.push(input.clone())
                }
            }
        } else {
            inputs.push(raw_input);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        config::{DataType, GlobalOptions, SinkConfig, SinkContext, SourceConfig, TransformConfig},
        shutdown::ShutdownSignal,
        sinks::{Healthcheck, VectorSink},
        sources::Source,
        transforms::Transform,
        Pipeline,
    };
    use async_trait::async_trait;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    struct MockSourceConfig;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct MockTransformConfig;

    #[derive(Debug, Serialize, Deserialize)]
    struct MockSinkConfig;

    #[async_trait]
    #[typetag::serde(name = "mock")]
    impl SourceConfig for MockSourceConfig {
        async fn build(
            &self,
            _name: &str,
            _globals: &GlobalOptions,
            _shutdown: ShutdownSignal,
            _out: Pipeline,
        ) -> crate::Result<Source> {
            unimplemented!()
        }

        fn source_type(&self) -> &'static str {
            "mock"
        }

        fn output_type(&self) -> DataType {
            DataType::Any
        }
    }

    #[async_trait]
    #[typetag::serde(name = "mock")]
    impl TransformConfig for MockTransformConfig {
        async fn build(&self, _globals: &GlobalOptions) -> crate::Result<Transform> {
            unimplemented!()
        }

        fn transform_type(&self) -> &'static str {
            "mock"
        }

        fn input_type(&self) -> DataType {
            DataType::Any
        }

        fn output_type(&self) -> DataType {
            DataType::Any
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
    fn wildcard_expansion() {
        let mut builder = ConfigBuilder::default();
        builder.add_source("foo1", MockSourceConfig);
        builder.add_source("foo2", MockSourceConfig);
        builder.add_source("bar", MockSourceConfig);
        builder.add_transform("foos", &["foo*"], MockTransformConfig);
        builder.add_sink("baz", &["foos*", "b*"], MockSinkConfig);
        builder.add_sink("quux", &["*"], MockSinkConfig);

        let config = builder.build().expect("build should succeed");

        assert_eq!(config.transforms["foos"].inputs, vec!["foo1", "foo2"]);
        assert_eq!(config.sinks["baz"].inputs, vec!["foos", "bar"]);
        assert_eq!(
            config.sinks["quux"].inputs,
            vec!["foo1", "foo2", "bar", "foos"]
        );
    }
}
