use std::collections::HashMap;

use snafu::Snafu;
use vector_lib::config::SourceOutput;

pub(super) use crate::schema::Definition;

use crate::{
    config::{ComponentKey, Config, OutputId, SinkOuter, TransformOutput},
    topology,
};

#[derive(Debug, Snafu)]
pub enum Error {
    ContainsNever,
}

/// The cache is used whilst building up the topology.
/// TODO: Describe more, especially why we have a bool in the key.
type Cache = HashMap<(bool, Vec<OutputId>), Vec<(OutputId, Definition)>>;

pub fn possible_definitions(
    inputs: &[OutputId],
    config: &dyn ComponentContainer,
    enrichment_tables: vector_lib::enrichment::TableRegistry,
    cache: &mut Cache,
) -> Result<Vec<(OutputId, Definition)>, Error> {
    if inputs.is_empty() {
        return Ok(vec![]);
    }

    // Try to get the definition from the cache.
    if let Some(definition) = cache.get(&(config.schema_enabled(), inputs.to_vec())) {
        return Ok(definition.clone());
    }

    let mut definitions = Vec::new();

    for input in inputs {
        let key = &input.component;

        // If the input is a source, the output is merged into the top-level schema.
        if let Ok(maybe_output) = config.source_output_for_port(key, &input.port) {
            let mut source_definition = input.with_definitions(
                maybe_output
                    .unwrap_or_else(|| {
                        unreachable!(
                            "source output mis-configured - output for port {:?} missing",
                            &input.port
                        )
                    })
                    .schema_definition(config.schema_enabled()),
            );

            if contains_never(&source_definition) {
                return Err(Error::ContainsNever);
            }

            definitions.append(&mut source_definition);
        }

        // If the input is a transform, the output is merged into the top-level schema
        if let Some(inputs) = config.transform_inputs(key) {
            let input_definitions =
                possible_definitions(inputs, config, enrichment_tables.clone(), cache)?;

            let mut transform_definition = input.with_definitions(
                config
                    .transform_output_for_port(
                        key,
                        &input.port,
                        enrichment_tables.clone(),
                        &input_definitions,
                    )
                    .expect("transform must exist - already found inputs")
                    .unwrap_or_else(|| {
                        unreachable!(
                            "transform output mis-configured - output for port {:?} missing",
                            &input.port
                        )
                    })
                    .schema_definitions(config.schema_enabled())
                    .values()
                    .cloned(),
            );

            if contains_never(&transform_definition) {
                return Err(Error::ContainsNever);
            }

            definitions.append(&mut transform_definition);
        }
    }

    Ok(definitions)
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
    enrichment_tables: vector_lib::enrichment::TableRegistry,
    inputs: &[OutputId],
    config: &dyn ComponentContainer,
    cache: &mut Cache,
) -> Result<Vec<(OutputId, Definition)>, Error> {
    // Try to get the definition from the cache.
    if let Some(definitions) = cache.get(&(config.schema_enabled(), inputs.to_vec())) {
        return Ok(definitions.clone());
    }

    let mut definitions: Vec<(OutputId, Definition)> = vec![];
    let mut merged_cache = HashMap::default();

    for input in inputs {
        let key = &input.component;

        // If the input is a source, it'll always have schema definition attached, even if it is an
        // "empty" schema.
        // We take the full schema definition regardless of `config.schema_enabled()`, the assumption
        // being that the receiving component will not be validating the schema if schema checking is
        // not enabled.
        if let Some(outputs) = config.source_outputs(key) {
            // After getting the source matching to the given input, we need to further narrow the
            // actual output of the source feeding into this input, and then get the definition
            // belonging to that output.
            let mut source_definitions =
                outputs
                    .into_iter()
                    .find_map(|output| {
                        if output.port == input.port {
                            Some(input.with_definitions(
                                output.schema_definition(config.schema_enabled()),
                            ))
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| {
                        // If we find no match, it means the topology is misconfigured. This is a fatal
                        // error, but other parts of the topology builder deal with this state.
                        unreachable!("source output mis-configured")
                    });

            if contains_never(&source_definitions) {
                return Err(Error::ContainsNever);
            }

            definitions.append(&mut source_definitions);

        // A transform can receive from multiple inputs, and each input needs to be expanded to
        // a new pipeline.
        } else if let Some(inputs) = config.transform_inputs(key) {
            let input_definitions =
                possible_definitions(inputs, config, enrichment_tables.clone(), &mut merged_cache)?;

            let mut transform_definition = config
                .transform_outputs(key, enrichment_tables.clone(), &input_definitions)
                .expect("already found inputs")
                .iter()
                .find_map(|output| {
                    if output.port == input.port {
                        Some(
                            input.with_definitions(
                                output
                                    .schema_definitions(config.schema_enabled())
                                    .values()
                                    .cloned(),
                            ),
                        )
                    } else {
                        None
                    }
                })
                // If we find no match, it means the topology is misconfigured. This is a fatal
                // error, but other parts of the topology builder deal with this state.
                .expect("transform output misconfigured");

            if contains_never(&transform_definition) {
                return Err(Error::ContainsNever);
            }

            // Append whatever number of additional pipelines we created to the existing
            // pipeline definitions.
            definitions.append(&mut transform_definition);
        }
    }

    cache.insert(
        (config.schema_enabled(), inputs.to_vec()),
        definitions.clone(),
    );

    Ok(definitions)
}

/// Returns a list of definitions from the given inputs.
/// Errors if any of the definitions are [`Kind::never`] implying that
/// an error condition has been reached.
pub(crate) fn input_definitions(
    inputs: &[OutputId],
    config: &Config,
    enrichment_tables: vector_lib::enrichment::TableRegistry,
    cache: &mut Cache,
) -> Result<Vec<(OutputId, Definition)>, Error> {
    if inputs.is_empty() {
        return Ok(vec![]);
    }

    if let Some(definitions) = cache.get(&(config.schema_enabled(), inputs.to_vec())) {
        return Ok(definitions.clone());
    }

    let mut definitions = Vec::new();

    for input in inputs {
        let key = &input.component;

        // If the input is a source we retrieve the definitions from the source
        // (there should only be one) and add it to the return.
        if let Ok(maybe_output) = config.source_output_for_port(key, &input.port) {
            let mut source_definitions = input.with_definitions(
                maybe_output
                    .unwrap_or_else(|| {
                        unreachable!(
                            "source output mis-configured - output for port {:?} missing",
                            &input.port
                        )
                    })
                    .schema_definition(config.schema_enabled()),
            );

            if contains_never(&source_definitions) {
                return Err(Error::ContainsNever);
            }

            definitions.append(&mut source_definitions);
        }

        // If the input is a transform we recurse to the upstream components to retrieve
        // their definitions and pass it through the transform to get the new definitions.
        if let Some(inputs) = config.transform_inputs(key) {
            let transform_definitions =
                input_definitions(inputs, config, enrichment_tables.clone(), cache)?;

            if contains_never(&transform_definitions) {
                return Err(Error::ContainsNever);
            }

            let mut transform_definitions = input.with_definitions(
                config
                    .transform_output_for_port(
                        key,
                        &input.port,
                        enrichment_tables.clone(),
                        &transform_definitions,
                    )
                    .expect("transform must exist")
                    .unwrap_or_else(|| {
                        unreachable!(
                            "transform output mis-configured - output for port {:?} missing",
                            &input.port
                        )
                    })
                    .schema_definitions(config.schema_enabled())
                    .values()
                    .cloned(),
            );

            if contains_never(&transform_definitions) {
                return Err(Error::ContainsNever);
            }

            definitions.append(&mut transform_definitions);
        }
    }

    Ok(definitions)
}

/// Checks if any of the definitions in the list contain `Kind::never()`. This
/// implies the definition cannot contain any output and thus we should stop
/// processing further.
fn contains_never(transform_definitions: &[(OutputId, Definition)]) -> bool {
    transform_definitions
        .iter()
        .any(|(_, definition)| definition.event_kind().is_never())
}

pub(super) fn validate_sink_expectations(
    key: &ComponentKey,
    sink: &SinkOuter<OutputId>,
    config: &topology::Config,
    enrichment_tables: vector_lib::enrichment::TableRegistry,
) -> Result<(), Vec<String>> {
    let mut errors = vec![];

    // Get the schema against which we need to validate the schemas of the components feeding into
    // this sink.
    let input = sink.inner.input();
    let requirement = input.schema_requirement();

    // Get all pipeline definitions feeding into this sink.
    let mut cache = HashMap::default();
    let definitions =
        match expanded_definitions(enrichment_tables, &sink.inputs, config, &mut cache) {
            Ok(definitions) => definitions,
            Err(err) => {
                errors.push(err.to_string());
                return Err(errors);
            }
        };

    // Validate each individual definition against the sink requirement.
    for (_output, definition) in definitions {
        if let Err(err) = requirement.validate(&definition, config.schema.validation) {
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

    fn source_outputs(&self, key: &ComponentKey) -> Option<Vec<SourceOutput>>;

    fn transform_inputs(&self, key: &ComponentKey) -> Option<&[OutputId]>;

    fn transform_outputs(
        &self,
        key: &ComponentKey,
        enrichment_tables: vector_lib::enrichment::TableRegistry,
        input_definitions: &[(OutputId, Definition)],
    ) -> Option<Vec<TransformOutput>>;

    /// Gets the transform output for the given port.
    ///
    /// Returns Err(()) if there is no transform with the given key
    /// Returns Some(None) if the source does not have an output for the port given
    #[allow(clippy::result_unit_err)]
    fn transform_output_for_port(
        &self,
        key: &ComponentKey,
        port: &Option<String>,
        enrichment_tables: vector_lib::enrichment::TableRegistry,
        input_definitions: &[(OutputId, Definition)],
    ) -> Result<Option<TransformOutput>, ()> {
        if let Some(outputs) = self.transform_outputs(key, enrichment_tables, input_definitions) {
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
    ) -> Result<Option<SourceOutput>, ()> {
        if let Some(outputs) = self.source_outputs(key) {
            Ok(get_source_output_for_port(outputs, port))
        } else {
            Err(())
        }
    }
}

fn get_output_for_port(
    outputs: Vec<TransformOutput>,
    port: &Option<String>,
) -> Option<TransformOutput> {
    outputs.into_iter().find(|output| &output.port == port)
}

fn get_source_output_for_port(
    outputs: Vec<SourceOutput>,
    port: &Option<String>,
) -> Option<SourceOutput> {
    outputs.into_iter().find(|output| &output.port == port)
}

impl ComponentContainer for Config {
    fn schema_enabled(&self) -> bool {
        self.schema.enabled
    }

    fn source_outputs(&self, key: &ComponentKey) -> Option<Vec<SourceOutput>> {
        self.source(key)
            .map(|source| source.inner.outputs(self.schema.log_namespace()))
    }

    fn transform_inputs(&self, key: &ComponentKey) -> Option<&[OutputId]> {
        self.transform(key).map(|transform| &transform.inputs[..])
    }

    fn transform_outputs(
        &self,
        key: &ComponentKey,
        enrichment_tables: vector_lib::enrichment::TableRegistry,
        input_definitions: &[(OutputId, Definition)],
    ) -> Option<Vec<TransformOutput>> {
        self.transform(key).map(|source| {
            source.inner.outputs(
                enrichment_tables,
                input_definitions,
                self.schema.log_namespace(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use indexmap::IndexMap;
    use similar_asserts::assert_eq;
    use vector_lib::config::{DataType, SourceOutput, TransformOutput};
    use vector_lib::lookup::owned_value_path;
    use vrl::value::Kind;

    use super::*;

    #[test]
    fn test_expanded_definition() {
        struct TestCase {
            inputs: Vec<(&'static str, Option<String>)>,
            sources: IndexMap<&'static str, Vec<SourceOutput>>,
            transforms: IndexMap<&'static str, (Vec<OutputId>, Vec<TransformOutput>)>,
            want: Vec<(OutputId, Definition)>,
        }

        impl ComponentContainer for TestCase {
            fn schema_enabled(&self) -> bool {
                true
            }

            fn source_outputs(&self, key: &ComponentKey) -> Option<Vec<SourceOutput>> {
                self.sources.get(key.id()).cloned()
            }

            fn transform_inputs(&self, key: &ComponentKey) -> Option<&[OutputId]> {
                self.transforms.get(key.id()).map(|v| v.0.as_slice())
            }

            fn transform_outputs(
                &self,
                key: &ComponentKey,
                _: vector_lib::enrichment::TableRegistry,
                _: &[(OutputId, Definition)],
            ) -> Option<Vec<TransformOutput>> {
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
                    sources: IndexMap::from([(
                        "foo",
                        vec![SourceOutput::new_maybe_logs(
                            DataType::all_bits(),
                            Definition::default_legacy_namespace(),
                        )],
                    )]),
                    transforms: IndexMap::default(),
                    want: vec![("foo".into(), Definition::default_legacy_namespace())],
                },
            ),
            (
                "single input, source with schema",
                TestCase {
                    inputs: vec![("source-foo", None)],
                    sources: IndexMap::from([(
                        "source-foo",
                        vec![SourceOutput::new_maybe_logs(
                            DataType::all_bits(),
                            Definition::empty_legacy_namespace().with_event_field(
                                &owned_value_path!("foo"),
                                Kind::integer().or_bytes(),
                                Some("foo bar"),
                            ),
                        )],
                    )]),
                    transforms: IndexMap::default(),
                    want: vec![(
                        "source-foo".into(),
                        Definition::empty_legacy_namespace().with_event_field(
                            &owned_value_path!("foo"),
                            Kind::integer().or_bytes(),
                            Some("foo bar"),
                        ),
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
                            vec![SourceOutput::new_maybe_logs(
                                DataType::all_bits(),
                                Definition::empty_legacy_namespace().with_event_field(
                                    &owned_value_path!("foo"),
                                    Kind::integer().or_bytes(),
                                    Some("foo bar"),
                                ),
                            )],
                        ),
                        (
                            "source-bar",
                            vec![SourceOutput::new_maybe_logs(
                                DataType::all_bits(),
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
                        (
                            "source-foo".into(),
                            Definition::empty_legacy_namespace().with_event_field(
                                &owned_value_path!("foo"),
                                Kind::integer().or_bytes(),
                                Some("foo bar"),
                            ),
                        ),
                        (
                            "source-bar".into(),
                            Definition::empty_legacy_namespace().with_event_field(
                                &owned_value_path!("foo"),
                                Kind::timestamp(),
                                Some("baz qux"),
                            ),
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
                            vec![SourceOutput::new_maybe_logs(
                                DataType::all_bits(),
                                Definition::empty_legacy_namespace().with_event_field(
                                    &owned_value_path!("foo"),
                                    Kind::boolean(),
                                    Some("foo"),
                                ),
                            )],
                        ),
                        (
                            "source-bar",
                            vec![SourceOutput::new_maybe_logs(
                                DataType::all_bits(),
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
                            vec![TransformOutput::new(
                                DataType::all_bits(),
                                [(
                                    "source-foo".into(),
                                    Definition::empty_legacy_namespace().with_event_field(
                                        &owned_value_path!("baz"),
                                        Kind::regex(),
                                        Some("baz"),
                                    ),
                                )]
                                .into(),
                            )],
                        ),
                    )]),
                    want: vec![
                        (
                            "source-bar".into(),
                            Definition::empty_legacy_namespace().with_event_field(
                                &owned_value_path!("bar"),
                                Kind::integer(),
                                Some("bar"),
                            ),
                        ),
                        (
                            "transform-baz".into(),
                            Definition::empty_legacy_namespace().with_event_field(
                                &owned_value_path!("baz"),
                                Kind::regex(),
                                Some("baz"),
                            ),
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
                            vec![SourceOutput::new_maybe_logs(
                                DataType::all_bits(),
                                Definition::empty_legacy_namespace().with_event_field(
                                    &owned_value_path!("source-1"),
                                    Kind::boolean(),
                                    Some("source-1"),
                                ),
                            )],
                        ),
                        (
                            "Source 2",
                            vec![SourceOutput::new_maybe_logs(
                                DataType::all_bits(),
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
                                vec![TransformOutput::new(
                                    DataType::all_bits(),
                                    [(
                                        "Source 1".into(),
                                        Definition::empty_legacy_namespace().with_event_field(
                                            &owned_value_path!("transform-1"),
                                            Kind::regex(),
                                            None,
                                        ),
                                    )]
                                    .into(),
                                )],
                            ),
                        ),
                        (
                            "Transform 2",
                            (
                                vec![OutputId::from("Source 2")],
                                vec![TransformOutput::new(
                                    DataType::all_bits(),
                                    [(
                                        "Source 2".into(),
                                        Definition::empty_legacy_namespace().with_event_field(
                                            &owned_value_path!("transform-2"),
                                            Kind::float().or_null(),
                                            Some("transform-2"),
                                        ),
                                    )]
                                    .into(),
                                )],
                            ),
                        ),
                        (
                            "Transform 3",
                            (
                                vec![OutputId::from("Source 2")],
                                vec![TransformOutput::new(
                                    DataType::all_bits(),
                                    [(
                                        "Source 2".into(),
                                        Definition::empty_legacy_namespace().with_event_field(
                                            &owned_value_path!("transform-3"),
                                            Kind::integer(),
                                            Some("transform-3"),
                                        ),
                                    )]
                                    .into(),
                                )],
                            ),
                        ),
                        (
                            "Transform 4",
                            (
                                vec![OutputId::from("Source 2")],
                                vec![TransformOutput::new(
                                    DataType::all_bits(),
                                    [(
                                        "Source 2".into(),
                                        Definition::empty_legacy_namespace().with_event_field(
                                            &owned_value_path!("transform-4"),
                                            Kind::timestamp().or_bytes(),
                                            Some("transform-4"),
                                        ),
                                    )]
                                    .into(),
                                )],
                            ),
                        ),
                        (
                            "Transform 5",
                            (
                                vec![OutputId::from("Transform 3"), OutputId::from("Transform 4")],
                                vec![TransformOutput::new(
                                    DataType::all_bits(),
                                    [(
                                        "Transform 3".into(),
                                        Definition::empty_legacy_namespace().with_event_field(
                                            &owned_value_path!("transform-5"),
                                            Kind::boolean(),
                                            Some("transform-5"),
                                        ),
                                    )]
                                    .into(),
                                )],
                            ),
                        ),
                    ]),
                    want: vec![
                        // Pipeline 1
                        (
                            "Transform 1".into(),
                            Definition::empty_legacy_namespace().with_event_field(
                                &owned_value_path!("transform-1"),
                                Kind::regex(),
                                None,
                            ),
                        ),
                        // Pipeline 2
                        (
                            "Transform 2".into(),
                            Definition::empty_legacy_namespace().with_event_field(
                                &owned_value_path!("transform-2"),
                                Kind::float().or_null(),
                                Some("transform-2"),
                            ),
                        ),
                        // Pipeline 3
                        (
                            "Transform 5".into(),
                            Definition::empty_legacy_namespace().with_event_field(
                                &owned_value_path!("transform-5"),
                                Kind::boolean(),
                                Some("transform-5"),
                            ),
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

            let got = expanded_definitions(
                vector_lib::enrichment::TableRegistry::default(),
                &inputs,
                &case,
                &mut HashMap::default(),
            )
            .unwrap();
            assert_eq!(got, case.want, "{}", title);
        }
    }
}
