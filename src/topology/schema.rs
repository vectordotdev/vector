use std::collections::HashMap;
use value::Kind;

pub(super) use crate::schema::Definition;

use crate::{
    config::{
        ComponentKey, Config, Output, OutputId, SinkConfig, SinkOuter, SourceConfig,
        TransformConfig,
    },
    topology,
};

/// Create a new [`Definition`] by recursively merging all provided inputs into a given component.
///
/// Recursion happens when one of the components inputs references a transform that has no
/// definition output of its own, in such a case, the definition output becomes the merged output
/// of that transform's inputs.
///
/// For example:
///
/// Source 1 [Definition 1] ->
/// Source 2 [Definition 2] -> Transform 1 []             -> [Definition 1 & 2]
/// Source 3 [Definition 3] -> Transform 2 [Definition 4] -> [Definition 4]     -> Sink
///
/// When asking for the merged definition feeding into `Sink`, `Transform 1` returns no definition
/// of its own, when asking for its schema definition. In this case the `merged_definition` method
/// recurses further back towards `Source 1` and `Source 2`, merging the two into a new definition
/// (marked as `[Definition 1 & 2]` above).
///
/// It then asks for the definition of `Transform 2`, which *does* defines its own definition,
/// named `Definition 4`, which overrides `Definition 3` feeding into `Transform 2`. In this case,
/// the `Sink` is only interested in `Definition 4`, and ignores `Definition 3`.
///
/// Finally, The merged definition (named `Definition 1 & 2`), and `Definition 4` are merged
/// together to produce the new `Definition` returned by this method.
pub fn merged_definition(
    inputs: &[OutputId],
    config: &dyn ComponentContainer,
    cache: &mut HashMap<(bool, Vec<OutputId>), Definition>,
) -> Definition {
    if inputs.is_empty() {
        return Definition::default_legacy_namespace();
    }

    // Try to get the definition from the cache.
    if let Some(definition) = cache.get(&(config.schema_enabled(), inputs.to_vec())) {
        return definition.clone();
    }

    let mut definition = Definition::new(Kind::never(), Kind::never(), []);

    for input in inputs {
        let key = &input.component;

        // If the input is a source, the output is merged into the top-level schema.
        // Not all sources contain a schema yet, in which case they use a default.
        if let Ok(maybe_output) = config.source_output_for_port(key, &input.port) {
            let source_definition = maybe_output
                .unwrap_or_else(|| {
                    unreachable!(
                        "source output mis-configured - output for port {:?} missing",
                        &input.port
                    )
                })
                .log_schema_definition
                .clone()
                // Schemas must be implemented for components that support the "Vector" namespace, so since
                // one doesn't exist here, we can assume it's using the default "legacy" namespace schema definition
                .unwrap_or_else(Definition::default_legacy_namespace);

            if config.schema_enabled() {
                definition = definition.merge(source_definition);
            } else {
                definition = definition.merge(Definition::default_for_namespace(
                    source_definition.log_namespaces(),
                ));
            }
        }
        // If the input is a transform, the output is merged into the top-level schema
        // Not all transforms contain a schema yet. If that's the case, it's assumed
        // that the transform doesn't modify the event schema, so it is passed through as-is (recursively)
        if let Some(inputs) = config.transform_inputs(key) {
            let merged_definition = merged_definition(inputs, config, cache);

            let transform_definition = config
                .transform_output_for_port(key, &input.port, &merged_definition)
                .expect("transform must exist - already found inputs")
                .unwrap_or_else(|| {
                    unreachable!(
                        "transform output mis-configured - output for port {:?} missing",
                        &input.port
                    )
                })
                .log_schema_definition
                .clone()
                .unwrap_or(merged_definition);

            if config.schema_enabled() {
                definition = definition.merge(transform_definition);
            } else {
                // Schemas must be implemented for components that support the "Vector" namespace, so since
                // one doesn't exist here, we can assume it's using the default "legacy" namespace schema definit
                definition = definition.merge(Definition::default_for_namespace(
                    transform_definition.log_namespaces(),
                ));
            }
        }
    }
    definition
}

/// Get a list of definitions from individual pipelines feeding into a component.
///
/// For example, given the following topology:
///
///   Source 1 -> Transform 1                ->
///   Source 2 -> Transform 2                ->
///            -> Transform 3 ->
///            -> Transform 4 -> Transform 5 -> Sink 1
///
/// In this example, if we ask for the definitions received by `Sink 1`, we'd receive four
/// definitions, one for each route leading into `Sink 1`, with the route going through `Transform
/// 5` being expanded into two individual routes (So1 -> T3 -> T5 -> Si1 AND So1 -> T4 -> T5 ->
/// Si1).
pub(super) fn expanded_definitions(
    inputs: &[OutputId],
    config: &dyn ComponentContainer,
    cache: &mut HashMap<(bool, Vec<OutputId>), Vec<Definition>>,
) -> Vec<Definition> {
    // Try to get the definition from the cache.
    if let Some(definitions) = cache.get(&(config.schema_enabled(), inputs.to_vec())) {
        return definitions.clone();
    }

    let mut definitions = vec![];
    let mut merged_cache = HashMap::default();

    for input in inputs {
        let key = &input.component;

        // If the input is a source, it'll always have schema definition attached, even if it is an
        // "empty" schema.
        if let Some(outputs) = config.source_outputs(key) {
            // After getting the source matching to the given input, we need to further narrow the
            // actual output of the source feeding into this input, and then get the definition
            // belonging to that output.
            let maybe_source_definition = outputs.iter().find_map(|output| {
                if output.port == input.port {
                    Some(
                        output
                            .log_schema_definition
                            .clone()
                            .unwrap_or_else(Definition::default_legacy_namespace),
                    )
                } else {
                    None
                }
            });

            let source_definition = match maybe_source_definition {
                Some(source_definition) => source_definition,
                // If we find no match, it means the topology is misconfigured. This is a fatal
                // error, but other parts of the topology builder deal with this state.
                None => unreachable!("source output mis-configured"),
            };

            definitions.push(source_definition);

        // A transform can receive from multiple inputs, and each input needs to be expanded to
        // a new pipeline.
        } else if let Some(inputs) = config.transform_inputs(key) {
            let merged_definition = merged_definition(inputs, config, &mut merged_cache);

            let maybe_transform_definition = config
                .transform_outputs(key, &merged_definition)
                .expect("already found inputs")
                .iter()
                .find_map(|output| {
                    if output.port == input.port {
                        Some(output.log_schema_definition.clone())
                    } else {
                        None
                    }
                })
                // If we find no match, it means the topology is misconfigured. This is a fatal
                // error, but other parts of the topology builder deal with this state.
                .expect("transform output misconfigured");

            // We need to iterate over the individual inputs of a transform, as we are expected to
            // expand each input into its own pipeline.
            for input in inputs {
                let mut expanded_definitions = match &maybe_transform_definition {
                    // If the transform defines its own schema definition, we no longer care about
                    // any upstream definitions, and use the transform definition instead.
                    Some(transform_definition) => vec![transform_definition.clone()],

                    // If the transform does not define its own schema definition, we need to
                    // recursively call this function in case upstream components expand into
                    // multiple pipelines.
                    None => expanded_definitions(&[input.clone()], config, cache),
                };

                // Append whatever number of additional pipelines we created to the existing
                // pipeline definitions.
                definitions.append(&mut expanded_definitions);
            }
        }
    }

    cache.insert(
        (config.schema_enabled(), inputs.to_vec()),
        definitions.clone(),
    );

    definitions
}

pub(super) fn validate_sink_expectations(
    key: &ComponentKey,
    sink: &SinkOuter<OutputId>,
    config: &topology::Config,
) -> Result<(), Vec<String>> {
    let mut errors = vec![];

    // Get the schema against which we need to validate the schemas of the components feeding into
    // this sink.
    let input = sink.inner.input();
    let requirement = input.schema_requirement();

    // Get all pipeline definitions feeding into this sink.
    let mut cache = HashMap::default();
    let definitions = expanded_definitions(&sink.inputs, config, &mut cache);

    // Validate each individual definition against the sink requirement.
    for definition in definitions {
        if let Err(err) = requirement.validate(&definition) {
            errors.append(
                &mut err
                    .errors()
                    .iter()
                    .cloned()
                    .map(|err| format!("schema error in component {}: {}", key, err))
                    .collect(),
            );
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(())
}

pub trait ComponentContainer {
    fn schema_enabled(&self) -> bool;

    fn source_outputs(&self, key: &ComponentKey) -> Option<Vec<Output>>;

    fn transform_inputs(&self, key: &ComponentKey) -> Option<&[OutputId]>;

    fn transform_outputs(
        &self,
        key: &ComponentKey,
        merged_definition: &Definition,
    ) -> Option<Vec<Output>>;

    /// Gets the transform output for the given port.
    ///
    /// Returns Err(()) if there is no transform with the given key
    /// Returns Some(None) if the source does not have an output for the port given
    #[allow(clippy::result_unit_err)]
    fn transform_output_for_port(
        &self,
        key: &ComponentKey,
        port: &Option<String>,
        merged_definition: &Definition,
    ) -> Result<Option<Output>, ()> {
        if let Some(outputs) = self.transform_outputs(key, merged_definition) {
            Ok(get_output_for_port(outputs, port))
        } else {
            Err(())
        }
    }

    /// Gets the source output for the given port.
    ///
    /// Returns Err(()) if there is no source with the given key
    /// Returns Some(None) if the source does not have an output for the port given
    #[allow(clippy::result_unit_err)]
    fn source_output_for_port(
        &self,
        key: &ComponentKey,
        port: &Option<String>,
    ) -> Result<Option<Output>, ()> {
        if let Some(outputs) = self.source_outputs(key) {
            Ok(get_output_for_port(outputs, port))
        } else {
            Err(())
        }
    }
}

fn get_output_for_port(outputs: Vec<Output>, port: &Option<String>) -> Option<Output> {
    outputs.into_iter().find(|output| &output.port == port)
}

impl ComponentContainer for Config {
    fn schema_enabled(&self) -> bool {
        self.schema.enabled
    }

    fn source_outputs(&self, key: &ComponentKey) -> Option<Vec<Output>> {
        self.source(key)
            .map(|source| source.inner.outputs(self.schema.log_namespace()))
    }

    fn transform_inputs(&self, key: &ComponentKey) -> Option<&[OutputId]> {
        self.transform(key).map(|transform| &transform.inputs[..])
    }

    fn transform_outputs(
        &self,
        key: &ComponentKey,
        merged_definition: &Definition,
    ) -> Option<Vec<Output>> {
        self.transform(key).map(|source| {
            source
                .inner
                .outputs(merged_definition, self.schema.log_namespace())
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use indexmap::IndexMap;
    use lookup::owned_value_path;
    use similar_asserts::assert_eq;
    use value::Kind;
    use vector_core::config::{DataType, Output};

    use super::*;

    #[test]
    fn test_merged_definition() {
        struct TestCase {
            inputs: Vec<(&'static str, Option<String>)>,
            sources: IndexMap<&'static str, Vec<Output>>,
            transforms: IndexMap<&'static str, Vec<Output>>,
            want: Definition,
        }

        impl ComponentContainer for TestCase {
            fn schema_enabled(&self) -> bool {
                true
            }

            fn source_outputs(&self, key: &ComponentKey) -> Option<Vec<Output>> {
                self.sources.get(key.id()).cloned()
            }

            fn transform_inputs(&self, _key: &ComponentKey) -> Option<&[OutputId]> {
                None
            }

            fn transform_outputs(
                &self,
                key: &ComponentKey,
                _merged_definition: &Definition,
            ) -> Option<Vec<Output>> {
                self.transforms.get(key.id()).cloned()
            }
        }

        for (title, case) in HashMap::from([
            (
                "no inputs",
                TestCase {
                    inputs: vec![],
                    sources: IndexMap::default(),
                    transforms: IndexMap::default(),
                    want: Definition::default_legacy_namespace(),
                },
            ),
            (
                "single input, source with empty schema",
                TestCase {
                    inputs: vec![("foo", None)],
                    sources: IndexMap::from([("foo", vec![Output::default(DataType::all())])]),
                    transforms: IndexMap::default(),
                    want: Definition::default_legacy_namespace(),
                },
            ),
            (
                "single input, source with schema",
                TestCase {
                    inputs: vec![("source-foo", None)],
                    sources: IndexMap::from([(
                        "source-foo",
                        vec![Output::default(DataType::all()).with_schema_definition(
                            Definition::empty_legacy_namespace().with_event_field(
                                &owned_value_path!("foo"),
                                Kind::integer().or_bytes(),
                                Some("foo bar"),
                            ),
                        )],
                    )]),
                    transforms: IndexMap::default(),
                    want: Definition::empty_legacy_namespace().with_event_field(
                        &owned_value_path!("foo"),
                        Kind::integer().or_bytes(),
                        Some("foo bar"),
                    ),
                },
            ),
            (
                "multiple inputs, sources with schema",
                TestCase {
                    inputs: vec![("source-foo", None), ("source-bar", None)],
                    sources: IndexMap::from([
                        (
                            "source-foo",
                            vec![Output::default(DataType::all()).with_schema_definition(
                                Definition::empty_legacy_namespace().with_event_field(
                                    &owned_value_path!("foo"),
                                    Kind::integer().or_bytes(),
                                    Some("foo bar"),
                                ),
                            )],
                        ),
                        (
                            "source-bar",
                            vec![Output::default(DataType::all()).with_schema_definition(
                                Definition::empty_legacy_namespace().with_event_field(
                                    &owned_value_path!("foo"),
                                    Kind::timestamp(),
                                    Some("baz qux"),
                                ),
                            )],
                        ),
                    ]),
                    transforms: IndexMap::default(),
                    want: Definition::empty_legacy_namespace()
                        .with_event_field(
                            &owned_value_path!("foo"),
                            Kind::integer().or_bytes().or_timestamp(),
                            Some("foo bar"),
                        )
                        .with_meaning("foo", "baz qux"),
                },
            ),
        ]) {
            let inputs = case
                .inputs
                .iter()
                .cloned()
                .map(|(key, port)| OutputId {
                    component: key.into(),
                    port,
                })
                .collect::<Vec<_>>();

            let got = merged_definition(&inputs, &case, &mut HashMap::default());
            assert_eq!(got, case.want, "{}", title);
        }
    }

    #[test]
    fn test_expanded_definition() {
        struct TestCase {
            inputs: Vec<(&'static str, Option<String>)>,
            sources: IndexMap<&'static str, Vec<Output>>,
            transforms: IndexMap<&'static str, (Vec<OutputId>, Vec<Output>)>,
            want: Vec<Definition>,
        }

        impl ComponentContainer for TestCase {
            fn schema_enabled(&self) -> bool {
                true
            }

            fn source_outputs(&self, key: &ComponentKey) -> Option<Vec<Output>> {
                self.sources.get(key.id()).cloned()
            }

            fn transform_inputs(&self, key: &ComponentKey) -> Option<&[OutputId]> {
                self.transforms.get(key.id()).map(|v| v.0.as_slice())
            }

            fn transform_outputs(
                &self,
                key: &ComponentKey,
                _merged_definition: &Definition,
            ) -> Option<Vec<Output>> {
                self.transforms.get(key.id()).cloned().map(|v| v.1)
            }
        }

        for (title, case) in HashMap::from([
            (
                "no inputs",
                TestCase {
                    inputs: vec![],
                    sources: IndexMap::default(),
                    transforms: IndexMap::default(),
                    want: vec![],
                },
            ),
            (
                "single input, source with default schema",
                TestCase {
                    inputs: vec![("foo", None)],
                    sources: IndexMap::from([("foo", vec![Output::default(DataType::all())])]),
                    transforms: IndexMap::default(),
                    want: vec![Definition::default_legacy_namespace()],
                },
            ),
            (
                "single input, source with schema",
                TestCase {
                    inputs: vec![("source-foo", None)],
                    sources: IndexMap::from([(
                        "source-foo",
                        vec![Output::default(DataType::all()).with_schema_definition(
                            Definition::empty_legacy_namespace().with_event_field(
                                &owned_value_path!("foo"),
                                Kind::integer().or_bytes(),
                                Some("foo bar"),
                            ),
                        )],
                    )]),
                    transforms: IndexMap::default(),
                    want: vec![Definition::empty_legacy_namespace().with_event_field(
                        &owned_value_path!("foo"),
                        Kind::integer().or_bytes(),
                        Some("foo bar"),
                    )],
                },
            ),
            (
                "multiple inputs, sources with schema",
                TestCase {
                    inputs: vec![("source-foo", None), ("source-bar", None)],
                    sources: IndexMap::from([
                        (
                            "source-foo",
                            vec![Output::default(DataType::all()).with_schema_definition(
                                Definition::empty_legacy_namespace().with_event_field(
                                    &owned_value_path!("foo"),
                                    Kind::integer().or_bytes(),
                                    Some("foo bar"),
                                ),
                            )],
                        ),
                        (
                            "source-bar",
                            vec![Output::default(DataType::all()).with_schema_definition(
                                Definition::empty_legacy_namespace().with_event_field(
                                    &owned_value_path!("foo"),
                                    Kind::timestamp(),
                                    Some("baz qux"),
                                ),
                            )],
                        ),
                    ]),
                    transforms: IndexMap::default(),
                    want: vec![
                        Definition::empty_legacy_namespace().with_event_field(
                            &owned_value_path!("foo"),
                            Kind::integer().or_bytes(),
                            Some("foo bar"),
                        ),
                        Definition::empty_legacy_namespace().with_event_field(
                            &owned_value_path!("foo"),
                            Kind::timestamp(),
                            Some("baz qux"),
                        ),
                    ],
                },
            ),
            (
                "transform overrides source",
                TestCase {
                    inputs: vec![("source-bar", None), ("transform-baz", None)],
                    sources: IndexMap::from([
                        (
                            "source-foo",
                            vec![Output::default(DataType::all()).with_schema_definition(
                                Definition::empty_legacy_namespace().with_event_field(
                                    &owned_value_path!("foo"),
                                    Kind::boolean(),
                                    Some("foo"),
                                ),
                            )],
                        ),
                        (
                            "source-bar",
                            vec![Output::default(DataType::all()).with_schema_definition(
                                Definition::empty_legacy_namespace().with_event_field(
                                    &owned_value_path!("bar"),
                                    Kind::integer(),
                                    Some("bar"),
                                ),
                            )],
                        ),
                    ]),
                    transforms: IndexMap::from([(
                        "transform-baz",
                        (
                            vec![OutputId::from("source-foo")],
                            vec![Output::default(DataType::all()).with_schema_definition(
                                Definition::empty_legacy_namespace().with_event_field(
                                    &owned_value_path!("baz"),
                                    Kind::regex(),
                                    Some("baz"),
                                ),
                            )],
                        ),
                    )]),
                    want: vec![
                        Definition::empty_legacy_namespace().with_event_field(
                            &owned_value_path!("bar"),
                            Kind::integer(),
                            Some("bar"),
                        ),
                        Definition::empty_legacy_namespace().with_event_field(
                            &owned_value_path!("baz"),
                            Kind::regex(),
                            Some("baz"),
                        ),
                    ],
                },
            ),
            //   Source 1 -> Transform 1                ->
            //   Source 2 -> Transform 2                ->
            //            -> Transform 3 ->
            //            -> Transform 4 -> Transform 5 -> Sink 1
            (
                "complex topology",
                TestCase {
                    inputs: vec![
                        ("Transform 1", None),
                        ("Transform 2", None),
                        ("Transform 5", None),
                    ],
                    sources: IndexMap::from([
                        (
                            "Source 1",
                            vec![Output::default(DataType::all()).with_schema_definition(
                                Definition::empty_legacy_namespace().with_event_field(
                                    &owned_value_path!("source-1"),
                                    Kind::boolean(),
                                    Some("source-1"),
                                ),
                            )],
                        ),
                        (
                            "Source 2",
                            vec![Output::default(DataType::all()).with_schema_definition(
                                Definition::empty_legacy_namespace().with_event_field(
                                    &owned_value_path!("source-2"),
                                    Kind::integer(),
                                    Some("source-2"),
                                ),
                            )],
                        ),
                    ]),
                    transforms: IndexMap::from([
                        (
                            "Transform 1",
                            (
                                vec![OutputId::from("Source 1")],
                                vec![Output::default(DataType::all()).with_schema_definition(
                                    Definition::empty_legacy_namespace().with_event_field(
                                        &owned_value_path!("transform-1"),
                                        Kind::regex(),
                                        None,
                                    ),
                                )],
                            ),
                        ),
                        (
                            "Transform 2",
                            (
                                vec![OutputId::from("Source 2")],
                                vec![Output::default(DataType::all()).with_schema_definition(
                                    Definition::empty_legacy_namespace().with_event_field(
                                        &owned_value_path!("transform-2"),
                                        Kind::float().or_null(),
                                        Some("transform-2"),
                                    ),
                                )],
                            ),
                        ),
                        (
                            "Transform 3",
                            (
                                vec![OutputId::from("Source 2")],
                                vec![Output::default(DataType::all()).with_schema_definition(
                                    Definition::empty_legacy_namespace().with_event_field(
                                        &owned_value_path!("transform-3"),
                                        Kind::integer(),
                                        Some("transform-3"),
                                    ),
                                )],
                            ),
                        ),
                        (
                            "Transform 4",
                            (
                                vec![OutputId::from("Source 2")],
                                vec![Output::default(DataType::all()).with_schema_definition(
                                    Definition::empty_legacy_namespace().with_event_field(
                                        &owned_value_path!("transform-4"),
                                        Kind::timestamp().or_bytes(),
                                        Some("transform-4"),
                                    ),
                                )],
                            ),
                        ),
                        (
                            "Transform 5",
                            (
                                vec![OutputId::from("Transform 3"), OutputId::from("Transform 4")],
                                vec![Output::default(DataType::all()).with_schema_definition(
                                    Definition::empty_legacy_namespace().with_event_field(
                                        &owned_value_path!("transform-5"),
                                        Kind::boolean(),
                                        Some("transform-5"),
                                    ),
                                )],
                            ),
                        ),
                    ]),
                    want: vec![
                        // Pipeline 1
                        Definition::empty_legacy_namespace().with_event_field(
                            &owned_value_path!("transform-1"),
                            Kind::regex(),
                            None,
                        ),
                        // Pipeline 2
                        Definition::empty_legacy_namespace().with_event_field(
                            &owned_value_path!("transform-2"),
                            Kind::float().or_null(),
                            Some("transform-2"),
                        ),
                        // Pipeline 3
                        Definition::empty_legacy_namespace().with_event_field(
                            &owned_value_path!("transform-5"),
                            Kind::boolean(),
                            Some("transform-5"),
                        ),
                        // Pipeline 4
                        Definition::empty_legacy_namespace().with_event_field(
                            &owned_value_path!("transform-5"),
                            Kind::boolean(),
                            Some("transform-5"),
                        ),
                    ],
                },
            ),
        ]) {
            let inputs = case
                .inputs
                .iter()
                .cloned()
                .map(|(key, port)| OutputId {
                    component: key.into(),
                    port,
                })
                .collect::<Vec<_>>();

            let got = expanded_definitions(&inputs, &case, &mut HashMap::default());
            assert_eq!(got, case.want, "{}", title);
        }
    }
}
