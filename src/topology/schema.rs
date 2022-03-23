use std::collections::HashMap;

pub(super) use crate::schema::Definition;

use crate::{config::OutputId, topology};

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
pub(super) fn merged_definition(
    inputs: &[OutputId],
    config: &topology::Config,
    cache: &mut HashMap<Vec<OutputId>, Definition>,
) -> Definition {
    // Try to get the definition from the cache.
    if let Some(definition) = cache.get(inputs) {
        return definition.clone();
    }

    let mut definition = Definition::empty();

    for input in inputs {
        let key = &input.component;

        // If the input is a source, it'll always have schema definition attached, even if it is an
        // "empty" schema.
        //
        // We merge this schema into the top-level schema.
        if let Some(source) = config.sources.get(key) {
            // After getting the source matching to the given input, we need to further narrow the
            // actual output of the source feeding into this input, and then get the definition
            // belonging to that output.
            let maybe_source_definition = source.inner.outputs().iter().find_map(|output| {
                if output.port == input.port {
                    // For sources, a `None` schema definition is equal to an "empty" definition.
                    Some(
                        output
                            .log_schema_definition
                            .clone()
                            .unwrap_or_else(Definition::empty),
                    )
                } else {
                    None
                }
            });

            let source_definition = match maybe_source_definition {
                Some(source_definition) => source_definition,
                // If we find no match, it means the topology is misconfigured. This is a fatal
                // error, but other parts of the topology builder deal with this state, so we
                // ignore it.
                None => continue,
            };

            definition = definition.merge(source_definition);

        // If the input is a transform, it _might_ define its own output schema, or it might not
        // change anything in the schema from its inputs, in which case we need to recursively get
        // the schemas of the transform inputs.
        } else if let Some(transform) = config.transforms.get(key) {
            let merged_definition = merged_definition(&transform.inputs, config, cache);

            // After getting the transform matching to the given input, we need to further narrow
            // the actual output of the transform feeding into this input, and then get the
            // definition belonging to that output.
            let maybe_transform_definition = transform
                .inner
                .outputs(&merged_definition)
                .iter()
                .find_map(|output| {
                    if output.port == input.port {
                        // For transforms, a `None` schema definition is equal to "pass-through merged
                        // input schemas".
                        output.log_schema_definition.clone()
                    } else {
                        None
                    }
                });

            let transform_definition = match maybe_transform_definition {
                Some(transform_definition) => transform_definition,
                // If we get no match, we need to recursively call this method for the inputs of
                // the given transform.
                None => merged_definition,
            };

            definition = definition.merge(transform_definition);
        }
    }

    cache.insert(inputs.to_vec(), definition.clone());

    definition
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use indexmap::IndexMap;
    use serde::{Deserialize, Serialize};
    use value::Kind;
    use vector_core::{
        config::{DataType, Input, Output},
        source::Source,
        transform::{Transform, TransformConfig, TransformContext},
    };

    use crate::config::{SourceConfig, SourceContext, SourceOuter, TransformOuter};

    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct MockComponent {
        #[serde(skip)]
        outputs: Vec<Output>,
    }

    #[async_trait::async_trait]
    #[typetag::serde(name = "mock_source")]
    impl SourceConfig for MockComponent {
        async fn build(&self, _: SourceContext) -> crate::Result<Source> {
            unimplemented!()
        }

        fn outputs(&self) -> Vec<Output> {
            self.outputs.clone()
        }

        fn source_type(&self) -> &'static str {
            unimplemented!()
        }

        fn can_acknowledge(&self) -> bool {
            false
        }
    }

    #[async_trait::async_trait]
    #[typetag::serde(name = "mock_transform")]
    impl TransformConfig for MockComponent {
        async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
            unimplemented!()
        }

        fn outputs(&self, _: &Definition) -> Vec<Output> {
            self.outputs.clone()
        }

        fn transform_type(&self) -> &'static str {
            unimplemented!()
        }

        fn input(&self) -> Input {
            unimplemented!()
        }
    }

    #[test]
    fn test_merged_definition() {
        struct TestCase {
            inputs: Vec<(&'static str, Option<String>)>,
            sources: IndexMap<&'static str, Vec<Output>>,
            transforms: IndexMap<&'static str, Vec<Output>>,
            want: Definition,
        }

        for (
            title,
            TestCase {
                inputs,
                sources,
                transforms,
                want,
            },
        ) in HashMap::from([
            (
                "no inputs",
                TestCase {
                    inputs: vec![],
                    sources: IndexMap::default(),
                    transforms: IndexMap::default(),
                    want: Definition::empty(),
                },
            ),
            (
                "single input, source with empty schema",
                TestCase {
                    inputs: vec![("foo", None)],
                    sources: IndexMap::from([("foo", vec![Output::default(DataType::all())])]),
                    transforms: IndexMap::default(),
                    want: Definition::empty(),
                },
            ),
            (
                "single input, source with schema",
                TestCase {
                    inputs: vec![("source-foo", None)],
                    sources: IndexMap::from([(
                        "source-foo",
                        vec![Output::default(DataType::all()).with_schema_definition(
                            Definition::empty().required_field(
                                "foo",
                                Kind::integer().or_bytes(),
                                Some("foo bar"),
                            ),
                        )],
                    )]),
                    transforms: IndexMap::default(),
                    want: Definition::empty().required_field(
                        "foo",
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
                                Definition::empty().required_field(
                                    "foo",
                                    Kind::integer().or_bytes(),
                                    Some("foo bar"),
                                ),
                            )],
                        ),
                        (
                            "source-bar",
                            vec![Output::default(DataType::all()).with_schema_definition(
                                Definition::empty().required_field(
                                    "foo",
                                    Kind::timestamp(),
                                    Some("baz qux"),
                                ),
                            )],
                        ),
                    ]),
                    transforms: IndexMap::default(),
                    want: Definition::empty()
                        .required_field("foo", Kind::integer().or_bytes(), Some("foo bar"))
                        .required_field("foo", Kind::timestamp(), Some("baz qux")),
                },
            ),
        ]) {
            let mut config = topology::Config::default();
            config.sources = sources
                .into_iter()
                .map(|(key, outputs)| (key.into(), SourceOuter::new(MockComponent { outputs })))
                .collect::<IndexMap<_, _>>();
            config.transforms = transforms
                .into_iter()
                .map(|(key, outputs)| (key.into(), TransformOuter::new(MockComponent { outputs })))
                .collect::<IndexMap<_, _>>();

            let inputs = inputs
                .into_iter()
                .map(|(key, port)| OutputId {
                    component: key.into(),
                    port,
                })
                .collect::<Vec<_>>();

            let got = merged_definition(&inputs, &config, &mut HashMap::default());
            assert_eq!(got, want, "{}", title);
        }
    }
}
