use crate::{
    conditions::Condition,
    config::{self, compiler::expand_macros, loading, SinkOuter, SourceOuter},
    event::{Event, Value},
    topology::{
        self,
        builder::{self, Pieces},
    },
};
use config::{
    graph::Graph, ComponentKey, Config, ConfigBuilder, ConfigDiff, ConfigPath, TestDefinition,
    TestInput, TestInputValue, TestOutput,
};
use futures_util::{stream::FuturesUnordered, StreamExt};
use indexmap::IndexMap;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::sync::{
    oneshot::{self, Receiver},
    Mutex,
};

use super::unit_test_components::{
    UnitTestSinkCheck, UnitTestSinkConfig, UnitTestSinkResult, UnitTestSourceConfig,
};

pub struct UnitTest {
    pub name: String,
    config: Config,
    diff: ConfigDiff,
    pieces: Pieces,
    sink_rxs: Vec<Receiver<UnitTestSinkResult>>,
}

pub async fn build_unit_tests_main(paths: &[ConfigPath]) -> Result<Vec<UnitTest>, Vec<String>> {
    config::init_log_schema(paths, false)?;

    let (config_builder, _) = loading::load_builder_from_paths(paths)?;

    build_unit_tests(config_builder).await
}

async fn build_unit_tests(mut config_builder: ConfigBuilder) -> Result<Vec<UnitTest>, Vec<String>> {
    let test_definitions = std::mem::take(&mut config_builder.tests);
    let mut tests = Vec::new();
    let mut build_errors = Vec::new();
    // todo: generate random id for inserted components

    for mut test_definition in test_definitions {
        let test_name = test_definition.name.clone();
        println!("test definition {:?}", test_definition);
        // Move the legacy single input into the inputs list if it exists
        let legacy_input = std::mem::take(&mut test_definition.input);
        if let Some(input) = legacy_input {
            test_definition.inputs.push(input);
        }
        match build_unit_test(test_definition, config_builder.clone()).await {
            Ok(test) => tests.push(test),
            Err(errors) => {
                let mut test_error = errors.join("\n");
                // Indent all line breaks
                test_error = test_error.replace("\n", "\n  ");
                test_error.insert_str(0, &format!("Failed to build test '{}':\n  ", test_name));
                build_errors.push(test_error);
            }
        }
    }

    if build_errors.is_empty() {
        Ok(tests)
    } else {
        Err(build_errors)
    }
}

// Remove any existing sources and sinks
// Retain only transform inputs that are other transforms
fn sanitize_config(config_builder: &mut ConfigBuilder) {
    config_builder.sources = Default::default();
    config_builder.sinks = Default::default();

    let all_valid_inputs = config_builder
        .transforms
        .iter()
        .flat_map(|(key, transform)| {
            // A transform and any of its named outputs
            let mut keys = vec![key.to_string()];
            keys.extend(
                transform
                    .inner
                    .named_outputs()
                    .iter()
                    .map(|port| format!("{}.{}", key, port))
                    .collect::<Vec<_>>(),
            );
            keys
        })
        .collect::<HashSet<_>>();

    for (_, transform) in config_builder.transforms.iter_mut() {
        let original_inputs = transform.inputs.clone().into_iter().collect::<HashSet<_>>();
        // Route is not covered by named outputs, so explicitly preserve inputs that start with a transform key + '.'
        let route_inputs = original_inputs
            .iter()
            .filter_map(|input| {
                let mut route_input = None;
                for valid_input in all_valid_inputs.iter() {
                    if input.starts_with::<&str>(format!("{}.", valid_input).as_ref()) {
                        route_input = Some(input.clone());
                        break;
                    }
                }
                route_input
            })
            .collect::<Vec<_>>();
        let mut new_inputs = original_inputs
            .intersection(&all_valid_inputs)
            .map(|input| input.clone())
            .collect::<Vec<_>>();
        new_inputs.extend(route_inputs);
        transform.inputs = new_inputs;
    }
}

async fn build_unit_test(
    test: TestDefinition,
    mut config_builder: ConfigBuilder,
) -> Result<UnitTest, Vec<String>> {
    sanitize_config(&mut config_builder);
    let mut build_errors = Vec::new();

    let available_insert_targets = config_builder
        .transforms
        .keys()
        .map(|key| key.clone())
        .collect::<HashSet<_>>();
    let inputs = build_and_validate_inputs(&test.inputs, &available_insert_targets)?;

    // Mapping from transform name to unit test source name
    let source_ids = available_insert_targets
        .iter()
        .map(|key| (key.clone(), format!("{}-{}", key, "source")))
        .collect::<HashMap<_, _>>();

    // Connect a test source to every transform
    let mut test_sources = IndexMap::new();
    for (key, transform) in config_builder.transforms.iter_mut() {
        let test_source_id = source_ids
            .get(key)
            .expect("Missing test source for a transform")
            .clone();
        transform.inputs.push(test_source_id.clone());

        test_sources.insert(key.clone(), UnitTestSourceConfig::default());
    }

    let insert_at_sources = inputs
        .keys()
        .map(|input| source_ids.get(input).unwrap().clone())
        .collect::<Vec<_>>();
    for (input, events) in inputs {
        let source_config = test_sources
            .get_mut(&input)
            .expect("Earlier validation of inputs was incorrect");
        source_config.events.extend(events);
    }
    let sources = test_sources
        .into_iter()
        .map(|(transform_key, source_config)| {
            let source_key: &str = source_ids
                .get(&transform_key)
                .expect("Corresponding source must exist")
                .as_ref();
            (
                ComponentKey::from(source_key),
                SourceOuter::new(source_config),
            )
        })
        .collect::<IndexMap<_, _>>();

    if test.outputs.is_empty() && test.no_outputs_from.is_empty() {
        build_errors.push(
            "unit test must contain at least one of `outputs` or `no_outputs_from`.".to_string(),
        );
        return Err(build_errors);
    }

    let available_extract_targets = config_builder
        .transforms
        .iter()
        .flat_map(|(key, transform)| {
            let mut extract_targets = vec![key.clone()];
            extract_targets.extend(
                transform
                    .inner
                    .named_outputs()
                    .iter()
                    .map(|port| ComponentKey::from(format!("{}.{}", key, port)))
                    .collect::<Vec<_>>(),
            );
            extract_targets
        })
        .collect::<HashSet<_>>();
    // Mapping from transform name to unit test sink name
    let sink_ids = available_extract_targets
        .iter()
        .map(|key| {
            (
                key.clone(),
                format!("{}-{}", key.to_string().replace(".", "-"), "sink"),
            )
        })
        .collect::<HashMap<_, _>>();
    let outputs = build_outputs(&test.outputs).unwrap_or_else(|errors| {
        build_errors.extend(errors);
        HashMap::new()
    });

    let mut sink_rxs = Vec::new();
    // Connect a sink to every transform output
    let mut test_sinks = IndexMap::new();
    for (transform_id, _) in sink_ids.iter() {
        let (tx, rx) = oneshot::channel();
        let sink_config = UnitTestSinkConfig {
            name: test.name.clone(),
            result_tx: Arc::new(Mutex::new(Some(tx))),
            check: UnitTestSinkCheck::Noop,
        };
        test_sinks.insert(transform_id.clone(), sink_config);
        sink_rxs.push(rx);
    }

    let mut extract_from_sinks = Vec::new();
    // Add checks to sinks associated with an extract_from
    for (transform_id, checks) in outputs {
        let sink_config = test_sinks
            .get_mut(&transform_id)
            .expect("Sink does not exist");
        sink_config.check = UnitTestSinkCheck::Checks(checks);

        let sink_id = sink_ids
            .get(&transform_id)
            .expect("Sink does not exist")
            .as_ref();
        extract_from_sinks.push(ComponentKey::from(sink_id));
    }

    // Add no outputs assertion to relevant sinks
    for transform_id in test.no_outputs_from {
        let sink_config = test_sinks
            .get_mut(&transform_id)
            .expect("Sink does not exist");
        sink_config.check = UnitTestSinkCheck::NoOutputs;

        let sink_id = sink_ids
            .get(&transform_id)
            .expect("Sink does not exist")
            .as_ref();
        extract_from_sinks.push(ComponentKey::from(sink_id));
    }

    let sinks = test_sinks
        .into_iter()
        .map(|(transform_id, sink_config)| {
            let sink_id = sink_ids
                .get(&transform_id)
                .expect("Sink does not exist")
                .as_ref();
            (
                ComponentKey::from(sink_id),
                SinkOuter::new(vec![transform_id.to_string()], Box::new(sink_config)),
            )
        })
        .collect::<IndexMap<_, _>>();

    config_builder.sources = sources;
    config_builder.sinks = sinks;
    let mut builder = config_builder.clone();
    let _ = expand_macros(&mut builder)?;

    // Check for an invalid input/output configuration wherein there is a
    // disconnect between insertions and extractions
    let graph = Graph::new(&builder.sources, &builder.transforms, &builder.sinks).unwrap();
    let insert_at_targets = test
        .inputs
        .iter()
        .map(|input| input.insert_at.to_string())
        .collect::<Vec<_>>();
    let valid_outputs = insert_at_sources
        .iter()
        .flat_map(|input| graph.get_leaves(&ComponentKey::from(input.as_ref())))
        .collect::<HashSet<_>>();

    for extract_from_target in extract_from_sinks {
        if !valid_outputs.contains(&extract_from_target) {
            build_errors.push(format!(
                "unable to complete topology between target transforms {:?} and output target '{}'",
                insert_at_targets,
                extract_from_target
                    .to_string()
                    .strip_suffix("-unit-test-sink")
                    .unwrap()
                    .replace("-", ".")
            ));
        }
    }
    if !build_errors.is_empty() {
        return Err(build_errors);
    }

    let config = config_builder.build()?;
    let diff = config::ConfigDiff::initial(&config);
    let pieces = builder::build_pieces(&config, &diff, HashMap::new()).await?;

    Ok(UnitTest {
        name: test.name,
        config,
        diff,
        pieces,
        sink_rxs,
    })
}

fn build_and_validate_inputs(
    test_inputs: &Vec<TestInput>,
    available_insert_targets: &HashSet<ComponentKey>,
) -> Result<HashMap<ComponentKey, Vec<Event>>, Vec<String>> {
    let mut inputs = HashMap::new();
    let mut errors = Vec::new();
    if test_inputs.is_empty() {
        errors.push("must specify at least one input.".to_string());
        return Err(errors);
    }

    for (index, input) in test_inputs.iter().enumerate() {
        if available_insert_targets.contains(&input.insert_at) {
            match build_input_event(&input) {
                Ok(input_event) => {
                    inputs
                        .entry(input.insert_at.clone())
                        .and_modify(|events: &mut Vec<Event>| {
                            events.push(input_event.clone());
                        })
                        .or_insert(vec![input_event]);
                }
                Err(error) => errors.push(error),
            }
        } else {
            errors.push(format!(
                "inputs[{}]: unable to locate target transform '{}'",
                index, input.insert_at
            ))
        }
    }

    if errors.is_empty() {
        Ok(inputs)
    } else {
        Err(errors)
    }
}

fn build_outputs(
    test_outputs: &Vec<TestOutput>,
) -> Result<HashMap<ComponentKey, Vec<Vec<Box<dyn Condition>>>>, Vec<String>> {
    let mut outputs = HashMap::new();
    let mut errors = Vec::new();

    for output in test_outputs {
        let mut conditions = Vec::new();
        for (index, condition) in output
            .conditions
            .clone()
            .unwrap_or(Vec::new())
            .iter()
            .enumerate()
        {
            match condition.build(&Default::default()) {
                Ok(condition) => conditions.push(condition),
                Err(error) => errors.push(format!(
                    "failed to create test condition '{}': {}",
                    index, error
                )),
            }
        }

        outputs
            .entry(output.extract_from.clone())
            .and_modify(|existing_conditions: &mut Vec<Vec<Box<dyn Condition>>>| {
                existing_conditions.push(conditions.clone())
            })
            .or_insert(vec![conditions]);
    }

    if errors.is_empty() {
        Ok(outputs)
    } else {
        Err(errors)
    }
}

impl UnitTest {
    pub async fn run(self) -> (Vec<String>, Vec<String>) {
        let (topology, _) = topology::start_validated(self.config, self.diff, self.pieces)
            .await
            .unwrap();
        let _ = topology.sources_finished().await;
        let _stop_complete = topology.stop();

        let mut in_flight = self.sink_rxs.into_iter().collect::<FuturesUnordered<_>>();

        let mut errors = Vec::new();
        while let Some(partial_result) = in_flight.next().await {
            let partial_result = partial_result.unwrap();
            if !partial_result.test_errors.is_empty() {
                errors.extend(partial_result.test_errors);
            }
        }

        (Vec::new(), errors)
    }
}

fn build_input_event(input: &TestInput) -> Result<Event, String> {
    match input.type_str.as_ref() {
        "raw" => match input.value.as_ref() {
            Some(v) => Ok(Event::from(v.clone())),
            None => Err("input type 'raw' requires the field 'value'".to_string()),
        },
        "log" => {
            if let Some(log_fields) = &input.log_fields {
                let mut event = Event::from("");
                for (path, value) in log_fields {
                    let value: Value = match value {
                        TestInputValue::String(s) => Value::from(s.to_owned()),
                        TestInputValue::Boolean(b) => Value::from(*b),
                        TestInputValue::Integer(i) => Value::from(*i),
                        TestInputValue::Float(f) => Value::from(*f),
                    };
                    event.as_mut_log().insert(path.to_owned(), value);
                }
                Ok(event)
            } else {
                Err("input type 'log' requires the field 'log_fields'".to_string())
            }
        }
        "metric" => {
            if let Some(metric) = &input.metric {
                Ok(Event::Metric(metric.clone()))
            } else {
                Err("input type 'metric' requires the field 'metric'".to_string())
            }
        }
        _ => Err(format!(
            "unrecognized input type '{}', expected one of: 'raw', 'log' or 'metric'",
            input.type_str
        )),
    }
}

#[cfg(all(test, feature = "transforms-add_fields", feature = "transforms-route"))]
mod tests {
    use indoc::indoc;

    use super::*;
    use crate::config::ConfigBuilder;

    #[tokio::test]
    async fn parse_no_input() {
        let config: ConfigBuilder = toml::from_str(indoc! {r#"
            [transforms.bar]
              inputs = ["foo"]
              type = "add_fields"
              [transforms.bar.fields]
                my_string_field = "string value"

              [[tests]]
                name = "broken test"

              [tests.input]
                insert_at = "foo"
                value = "nah this doesnt matter"

              [[tests.outputs]]
                extract_from = "bar"
                [[tests.outputs.conditions]]
                  type = "vrl"
                  source = ""
        "#})
        .unwrap();

        let errs = build_unit_tests(config).await.err().unwrap();
        assert_eq!(
            errs,
            vec![indoc! {r#"
                Failed to build test 'broken test':
                  inputs[0]: unable to locate target transform 'foo'"#}
            .to_owned(),]
        );

        let config: ConfigBuilder = toml::from_str(indoc! {r#"
            [transforms.bar]
              inputs = ["foo"]
              type = "add_fields"
              [transforms.bar.fields]
                my_string_field = "string value"

              [[tests]]
                name = "broken test"

              [[tests.inputs]]
                insert_at = "bar"
                value = "nah this doesnt matter"

              [[tests.inputs]]
                insert_at = "foo"
                value = "nah this doesnt matter"

              [[tests.outputs]]
                extract_from = "bar"
                [[tests.outputs.conditions]]
                  type = "vrl"
                  source = ""
        "#})
        .unwrap();

        let errs = build_unit_tests(config).await.err().unwrap();
        assert_eq!(
            errs,
            vec![indoc! {r#"
                Failed to build test 'broken test':
                  inputs[1]: unable to locate target transform 'foo'"#}
            .to_owned(),]
        );
    }

    #[tokio::test]
    async fn parse_no_test_input() {
        let config: ConfigBuilder = toml::from_str(indoc! {r#"
            [transforms.bar]
              inputs = ["foo"]
              type = "add_fields"
              [transforms.bar.fields]
                my_string_field = "string value"

              [[tests]]
                name = "broken test"

              [[tests.outputs]]
                extract_from = "bar"
                [[tests.outputs.conditions]]
                  type = "vrl"
                  source = ""
        "#})
        .unwrap();

        let errs = build_unit_tests(config).await.err().unwrap();
        assert_eq!(
            errs,
            vec![indoc! {r#"
                Failed to build test 'broken test':
                  must specify at least one input."#}
            .to_owned(),]
        );
    }

    #[tokio::test]
    async fn parse_no_outputs() {
        let config: ConfigBuilder = toml::from_str(indoc! {r#"
            [transforms.foo]
              inputs = ["ignored"]
              type = "add_fields"
              [transforms.foo.fields]
                my_string_field = "string value"

              [[tests]]
                name = "broken test"

              [tests.input]
                insert_at = "foo"
                value = "nah this doesnt matter"
        "#})
        .unwrap();

        let errs = build_unit_tests(config).await.err().unwrap();
        assert_eq!(
            errs,
            vec![indoc! {r#"
                Failed to build test 'broken test':
                  unit test must contain at least one of `outputs` or `no_outputs_from`."#}
            .to_owned(),]
        );
    }

    #[tokio::test]
    async fn parse_broken_topology() {
        let config: ConfigBuilder = toml::from_str(indoc! {r#"
            [transforms.foo]
              inputs = ["something"]
              type = "add_fields"
              [transforms.foo.fields]
                foo_field = "string value"

            [transforms.nah]
              inputs = ["ignored"]
              type = "add_fields"
              [transforms.nah.fields]
                new_field = "string value"

            [transforms.baz]
              inputs = ["bar"]
              type = "add_fields"
              [transforms.baz.fields]
                baz_field = "string value"

            [transforms.quz]
              inputs = ["bar"]
              type = "add_fields"
              [transforms.quz.fields]
                quz_field = "string value"

            [[tests]]
              name = "broken test"

              [tests.input]
                insert_at = "foo"
                value = "nah this doesnt matter"

              [[tests.outputs]]
                extract_from = "baz"
                [[tests.outputs.conditions]]
                  type = "vrl"
                  source = """
                    assert_eq!(.message, "not this")
                  """

              [[tests.outputs]]
                extract_from = "quz"
                [[tests.outputs.conditions]]
                  type = "vrl"
                  source = """
                    assert_eq!(.message, "not this")
                  """

            [[tests]]
              name = "broken test 2"

              [tests.input]
                insert_at = "nope"
                value = "nah this doesnt matter"

              [[tests.outputs]]
                extract_from = "quz"
                [[tests.outputs.conditions]]
                  type = "vrl"
                  source = """
                    assert_eq!(.message, "not this")
                  """

            [[tests]]
              name = "broken test 3"

              [[tests.inputs]]
                insert_at = "foo"
                value = "nah this doesnt matter"

              [[tests.inputs]]
                insert_at = "nah"
                value = "nah this doesnt matter"

              [[tests.outputs]]
                extract_from = "baz"
                [[tests.outputs.conditions]]
                  type = "vrl"
                  source = """
                    assert_eq!(.message, "not this")
                  """

              [[tests.outputs]]
                extract_from = "quz"
                [[tests.outputs.conditions]]
                  type = "vrl"
                  source = """
                    assert_eq!(.message, "not this")
                  """
        "#})
        .unwrap();

        let errs = build_unit_tests(config).await.err().unwrap();
        assert_eq!(
            errs,
            vec![
                r#"Failed to build test 'broken test':
  unable to complete topology between target transforms ["foo"] and output target 'baz'
  unable to complete topology between target transforms ["foo"] and output target 'quz'"#
                    .to_owned(),
                r#"Failed to build test 'broken test 2':
  inputs[0]: unable to locate target transform 'nope'"#
                    .to_owned(),
                r#"Failed to build test 'broken test 3':
  unable to complete topology between target transforms ["foo", "nah"] and output target 'baz'
  unable to complete topology between target transforms ["foo", "nah"] and output target 'quz'"#
                    .to_owned(),
            ]
        );
    }

    #[tokio::test]
    async fn parse_bad_input_event() {
        let config: ConfigBuilder = toml::from_str(indoc! {r#"
            [transforms.foo]
              inputs = ["ignored"]
              type = "add_fields"
              [transforms.foo.fields]
                my_string_field = "string value"

              [[tests]]
                name = "broken test"

              [tests.input]
                insert_at = "foo"
                type = "nah"
                value = "nah this doesnt matter"

              [[tests.outputs]]
                extract_from = "foo"
                [[tests.outputs.conditions]]
                  type = "vrl"
                  source = ""
        "#})
        .unwrap();

        let errs = build_unit_tests(config).await.err().unwrap();
        assert_eq!(
            errs,
            vec![indoc! {r#"
                Failed to build test 'broken test':
                  unrecognized input type 'nah', expected one of: 'raw', 'log' or 'metric'"#}
            .to_owned(),]
        );
    }

    #[tokio::test]
    async fn test_success_multi_inputs() {
        let config: ConfigBuilder = toml::from_str(indoc! {r#"
            [transforms.foo]
              inputs = ["ignored"]
              type = "add_fields"
              [transforms.foo.fields]
                new_field = "string value"

            [transforms.foo_two]
              inputs = ["ignored"]
              type = "add_fields"
              [transforms.foo_two.fields]
                new_field_two = "second string value"

            [transforms.bar]
              inputs = ["foo", "foo_two"]
              type = "add_fields"
              [transforms.bar.fields]
                second_new_field = "also a string value"

            [transforms.baz]
              inputs = ["bar"]
              type = "add_fields"
              [transforms.baz.fields]
                third_new_field = "also also a string value"

            [[tests]]
              name = "successful test"

              [[tests.inputs]]
                insert_at = "foo"
                value = "nah this doesnt matter"

              [[tests.inputs]]
                insert_at = "foo_two"
                value = "nah this also doesnt matter"

              [[tests.outputs]]
                extract_from = "foo"
                [[tests.outputs.conditions]]
                  type = "vrl"
                  source = """
                    assert_eq!(.new_field, "string value")
                    assert_eq!(.message, "nah this doesnt matter")
                  """

              [[tests.outputs]]
                extract_from = "foo_two"
                [[tests.outputs.conditions]]
                  type = "vrl"
                  source = """
                    assert_eq!(.new_field_two, "second string value")
                    assert_eq!(.message, "nah this also doesnt matter")
                  """

              [[tests.outputs]]
                extract_from = "bar"
                [[tests.outputs.conditions]]
                  type = "vrl"
                  source = """
                    assert_eq!(.new_field, "string value")
                    assert_eq!(.second_new_field, "also a string value")
                    assert_eq!(.message, "nah this doesnt matter")
                  """
                [[tests.outputs.conditions]]
                  type = "vrl"
                  source = """
                    assert_eq!(.new_field_two, "second string value")
                    assert_eq!(.second_new_field, "also a string value")
                    assert_eq!(.message, "nah this also doesnt matter")
                  """

              [[tests.outputs]]
                extract_from = "baz"
                [[tests.outputs.conditions]]
                  type = "vrl"
                  source = """
                    assert_eq!(.new_field, "string value")
                    assert_eq!(.second_new_field, "also a string value")
                    assert_eq!(.third_new_field, "also also a string value")
                    assert_eq!(.message, "nah this doesnt matter")
                  """
                [[tests.outputs.conditions]]
                  type = "vrl"
                  source = """
                    assert_eq!(.new_field_two, "second string value")
                    assert_eq!(.second_new_field, "also a string value")
                    assert_eq!(.third_new_field, "also also a string value")
                    assert_eq!(.message, "nah this also doesnt matter")
                  """
        "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_success() {
        let config: ConfigBuilder = toml::from_str(indoc! {r#"
            [transforms.foo]
              inputs = ["ignored"]
              type = "add_fields"
              [transforms.foo.fields]
                new_field = "string value"

            [transforms.bar]
              inputs = ["foo"]
              type = "add_fields"
              [transforms.bar.fields]
                second_new_field = "also a string value"

            [transforms.baz]
              inputs = ["bar"]
              type = "add_fields"
              [transforms.baz.fields]
                third_new_field = "also also a string value"

            [[tests]]
              name = "successful test"

              [tests.input]
                insert_at = "foo"
                value = "nah this doesnt matter"

              [[tests.outputs]]
                extract_from = "foo"
                [[tests.outputs.conditions]]
                  type = "vrl"
                  source = """
                    assert_eq!(.new_field, "string value")
                    assert_eq!(.message, "nah this doesnt matter")
                  """

              [[tests.outputs]]
                extract_from = "bar"
                [[tests.outputs.conditions]]
                  type = "vrl"
                  source = """
                    assert_eq!(.new_field, "string value")
                    assert_eq!(.second_new_field, "also a string value")
                    assert_eq!(.message, "nah this doesnt matter")
                  """

              [[tests.outputs]]
                extract_from = "baz"
                [[tests.outputs.conditions]]
                  type = "vrl"
                  source = """
                    assert_eq!(.new_field, "string value")
                    assert_eq!(.second_new_field, "also a string value")
                    assert_eq!(.third_new_field, "also also a string value")
                    assert_eq!(.message, "nah this doesnt matter")
                  """
        "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_route() {
        let config: ConfigBuilder = toml::from_str(indoc! {r#"
              [transforms.foo]
                inputs = ["ignored"]
                type = "route"
                  [transforms.foo.route]
                  first = '.message == "test swimlane 1"'
                  second = '.message == "test swimlane 2"'

              [transforms.bar]
                inputs = ["foo.first"]
                type = "add_fields"
                [transforms.bar.fields]
                  new_field = "new field added"

              [[tests]]
                name = "successful route test 1"

                [tests.input]
                  insert_at = "foo"
                  value = "test swimlane 1"

                [[tests.outputs]]
                  extract_from = "foo.first"
                  [[tests.outputs.conditions]]
                    type = "vrl"
                    source = """
                        assert_eq!(.message, "test swimlane 1")
                    """

                [[tests.outputs]]
                  extract_from = "bar"
                  [[tests.outputs.conditions]]
                    type = "vrl"
                    source = """
                        assert_eq!(.message, "test swimlane 1")
                        assert_eq!(.new_field, "new field added")
                    """

              [[tests]]
                name = "successful route test 2"

                [tests.input]
                  insert_at = "foo"
                  value = "test swimlane 2"

                [[tests.outputs]]
                  extract_from = "foo.second"
                  [[tests.outputs.conditions]]
                    type = "vrl"
                    source = """
                        assert_eq!(.message, "test swimlane 2")
                    """
          "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_fail_no_outputs() {
        let config: ConfigBuilder = toml::from_str(indoc! {r#"
              [transforms.foo]
                inputs = [ "TODO" ]
                type = "filter"
                [transforms.foo.condition]
                  type = "vrl"
                  source = """
                    .not_exist == "not_value"
                  """

                [[tests]]
                  name = "check_no_outputs"
                  [tests.input]
                    insert_at = "foo"
                    type = "raw"
                    value = "test value"

                  [[tests.outputs]]
                    extract_from = "foo"
                    [[tests.outputs.conditions]]
                      type = "vrl"
                      source = """
                        assert_eq!(.message, "test value")
                      """
          "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert_ne!(tests.remove(0).run().await.1, Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_fail_two_output_events() {
        let config: ConfigBuilder = toml::from_str(indoc! {r#"
              [transforms.foo]
                inputs = [ "TODO" ]
                type = "add_fields"
                [transforms.foo.fields]
                  foo = "new field 1"

              [transforms.bar]
                inputs = [ "foo" ]
                type = "add_fields"
                [transforms.bar.fields]
                  bar = "new field 2"

              [transforms.baz]
                inputs = [ "foo" ]
                type = "add_fields"
                [transforms.baz.fields]
                  baz = "new field 3"

              [transforms.boo]
                inputs = [ "bar", "baz" ]
                type = "add_fields"
                [transforms.boo.fields]
                  boo = "new field 4"

              [[tests]]
                name = "check_multi_payloads"

                [tests.input]
                  insert_at = "foo"
                  type = "raw"
                  value = "first"

                [[tests.outputs]]
                  extract_from = "boo"

                  [[tests.outputs.conditions]]
                    type = "vrl"
                    source = """
                      assert_eq!(.baz, "new field 3")
                    """

                  [[tests.outputs.conditions]]
                    type = "vrl"
                    source = """
                      assert_eq!(.bar, "new field 2")
                    """

              [[tests]]
                name = "check_multi_payloads_bad"

                [tests.input]
                  insert_at = "foo"
                  type = "raw"
                  value = "first"

                [[tests.outputs]]
                  extract_from = "boo"

                  [[tests.outputs.conditions]]
                    type = "vrl"
                    source = """
                        assert_eq!(.baz, "new field 3")
                        assert_eq!(.bar, "new field 2")
                    """
          "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
        assert_ne!(tests.remove(0).run().await.1, Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_no_outputs_from() {
        let config: ConfigBuilder = toml::from_str(indoc! {r#"
              [transforms.foo]
                inputs = [ "ignored" ]
                type = "filter"
                [transforms.foo.condition]
                  type = "vrl"
                  source = """
                    .message == "foo"
                  """

              [[tests]]
                name = "check_no_outputs_from_succeeds"
                no_outputs_from = [ "foo" ]

                [tests.input]
                  insert_at = "foo"
                  type = "raw"
                  value = "not foo at all"

              [[tests]]
                name = "check_no_outputs_from_fails"
                no_outputs_from = [ "foo" ]

                [tests.input]
                  insert_at = "foo"
                  type = "raw"
                  value = "foo"
          "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
        assert_ne!(tests.remove(0).run().await.1, Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_no_outputs_from_chained() {
        let config: ConfigBuilder = toml::from_str(indoc! { r#"
              [transforms.foo]
                inputs = [ "ignored" ]
                type = "filter"
                [transforms.foo.condition]
                  type = "vrl"
                  source = """
                    .message == "foo"
                  """

              [transforms.bar]
                inputs = [ "foo" ]
                type = "add_fields"
                [transforms.bar.fields]
                  bar = "new field"

              [[tests]]
                name = "check_no_outputs_from_succeeds"
                no_outputs_from = [ "bar" ]

                [tests.input]
                  insert_at = "foo"
                  type = "raw"
                  value = "not foo at all"

              [[tests]]
                name = "check_no_outputs_from_fails"
                no_outputs_from = [ "bar" ]

                [tests.input]
                  insert_at = "foo"
                  type = "raw"
                  value = "foo"
          "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
        assert_ne!(tests.remove(0).run().await.1, Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_log_input() {
        let config: ConfigBuilder = toml::from_str(indoc! { r#"
              [transforms.foo]
                inputs = ["ignored"]
                type = "add_fields"
                [transforms.foo.fields]
                  new_field = "string value"

              [[tests]]
                name = "successful test with log event"

                [tests.input]
                  insert_at = "foo"
                  type = "log"
                  [tests.input.log_fields]
                    message = "this is the message"
                    int_val = 5
                    bool_val = true

                [[tests.outputs]]
                  extract_from = "foo"
                  [[tests.outputs.conditions]]
                    type = "vrl"
                    source = """
                        assert_eq!(.new_field, "string value")
                        assert_eq!(.message, "this is the message")
                        assert_eq!(.message, "this is the message")
                        assert!(.bool_val)
                        assert_eq!(.int_val, 5)
                    """
          "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_metric_input() {
        let config: ConfigBuilder = toml::from_str(indoc! { r#"
              [transforms.foo]
                inputs = ["ignored"]
                type = "add_tags"
                [transforms.foo.tags]
                  new_tag = "new value added"

              [[tests]]
                name = "successful test with metric event"

                [tests.input]
                  insert_at = "foo"
                  type = "metric"
                  [tests.input.metric]
                    kind = "incremental"
                    name = "foometric"
                    [tests.input.metric.tags]
                      tagfoo = "valfoo"
                    [tests.input.metric.counter]
                      value = 100.0

                [[tests.outputs]]
                  extract_from = "foo"
                  [[tests.outputs.conditions]]
                    type = "vrl"
                    source = """
                        assert_eq!(.tags.tagfoo, "valfoo")
                        assert_eq!(.tags.new_tag, "new value added")
                    """
          "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_success_over_gap() {
        let config: ConfigBuilder = toml::from_str(indoc! { r#"
              [transforms.foo]
                inputs = ["ignored"]
                type = "add_fields"
                [transforms.foo.fields]
                  new_field = "string value"

              [transforms.bar]
                inputs = ["foo"]
                type = "add_fields"
                [transforms.bar.fields]
                  second_new_field = "also a string value"

              [transforms.baz]
                inputs = ["bar"]
                type = "add_fields"
                [transforms.baz.fields]
                  third_new_field = "also also a string value"

              [[tests]]
                name = "successful test"

                [tests.input]
                  insert_at = "foo"
                  value = "nah this doesnt matter"

                [[tests.outputs]]
                  extract_from = "baz"
                  [[tests.outputs.conditions]]
                    type = "vrl"
                    source = """
                        assert_eq!(.new_field, "string value")
                        assert_eq!(.second_new_field, "also a string value")
                        assert_eq!(.third_new_field, "also also a string value")
                        assert_eq!(.message, "nah this doesnt matter")
                    """
          "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_success_tree() {
        let config: ConfigBuilder = toml::from_str(indoc! { r#"
              [transforms.ignored]
                inputs = ["also_ignored"]
                type = "add_fields"
                [transforms.ignored.fields]
                  not_field = "string value"

              [transforms.foo]
                inputs = ["ignored"]
                type = "add_fields"
                [transforms.foo.fields]
                  new_field = "string value"

              [transforms.bar]
                inputs = ["foo"]
                type = "add_fields"
                [transforms.bar.fields]
                  second_new_field = "also a string value"

              [transforms.baz]
                inputs = ["foo"]
                type = "add_fields"
                [transforms.baz.fields]
                  second_new_field = "also also a string value"

              [[tests]]
                name = "successful test"

                [tests.input]
                  insert_at = "foo"
                  value = "nah this doesnt matter"

                [[tests.outputs]]
                  extract_from = "bar"
                  [[tests.outputs.conditions]]
                    type = "vrl"
                    source = """
                        assert_eq!(.new_field, "string value")
                        assert_eq!(.second_new_field, "also a string value")
                        assert_eq!(.message, "nah this doesnt matter")
                    """

                [[tests.outputs]]
                  extract_from = "baz"
                  [[tests.outputs.conditions]]
                    type = "vrl"
                    source = """
                        assert_eq!(.new_field, "string value")
                        assert_eq!(.second_new_field, "also also a string value")
                        assert_eq!(.message, "nah this doesnt matter")
                    """
          "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
    }

    //   #[tokio::test]
    //   async fn test_fails() {
    //       let config: ConfigBuilder = toml::from_str(indoc! { r#"
    //           [transforms.foo]
    //             inputs = ["ignored"]
    //             type = "remove_fields"
    //             fields = ["timestamp"]

    //           [transforms.bar]
    //             inputs = ["foo"]
    //             type = "add_fields"
    //             [transforms.bar.fields]
    //               second_new_field = "also a string value"

    //           [transforms.baz]
    //             inputs = ["bar"]
    //             type = "add_fields"
    //             [transforms.baz.fields]
    //               third_new_field = "also also a string value"

    //           [[tests]]
    //             name = "failing test"

    //             [tests.input]
    //               insert_at = "foo"
    //               value = "nah this doesnt matter"

    //             [[tests.outputs]]
    //               extract_from = "foo"
    //               [[tests.outputs.conditions]]
    //                 type = "check_fields"
    //                 "message.equals" = "nah this doesnt matter"

    //             [[tests.outputs]]
    //               extract_from = "bar"
    //               [[tests.outputs.conditions]]
    //                 type = "check_fields"
    //                 "message.equals" = "not this"
    //               [[tests.outputs.conditions]]
    //                 type = "check_fields"
    //                 "second_new_field.equals" = "and not this"

    //           [[tests]]
    //             name = "another failing test"

    //             [tests.input]
    //               insert_at = "foo"
    //               value = "also this doesnt matter"

    //             [[tests.outputs]]
    //               extract_from = "foo"
    //               [[tests.outputs.conditions]]
    //                 type = "check_fields"
    //                 "message.equals" = "also this doesnt matter"

    //             [[tests.outputs]]
    //               extract_from = "baz"
    //               [[tests.outputs.conditions]]
    //                 type = "check_fields"
    //                 "second_new_field.equals" = "nope not this"
    //                 "third_new_field.equals" = "and not this"
    //                 "message.equals" = "also this doesnt matter"
    //       "#})
    //       .unwrap();

    //       let mut tests = build_unit_tests(config).await.unwrap();
    //       assert_ne!(tests[0].run().1, Vec::<String>::new());
    //       assert_ne!(tests[1].run().1, Vec::<String>::new());
    //       // TODO: The json representations are randomly ordered so these checks
    //       // don't always pass:
    //       /*
    //               assert_eq!(
    //                   tests[0].run().1,
    //                   vec![r#"check transform 'bar' failed conditions:
    //         condition[0]: predicates failed: [ message.equals: 'not this' ]
    //         condition[1]: predicates failed: [ second_new_field.equals: 'and not this' ]
    //       payloads (JSON encoded):
    //         input: {"message":"nah this doesnt matter"}
    //         output: {"message":"nah this doesnt matter","second_new_field":"also a string value"}"#.to_owned(),
    //                   ]);
    //               assert_eq!(
    //                   tests[1].run().1,
    //                   vec![r#"check transform 'baz' failed conditions:
    //         condition[0]: predicates failed: [ second_new_field.equals: 'nope not this', third_new_field.equals: 'and not this' ]
    //       payloads (JSON encoded):
    //         input: {"second_new_field":"also a string value","message":"also this doesnt matter"}
    //         output: {"third_new_field":"also also a string value","second_new_field":"also a string value","message":"also this doesnt matter"}"#.to_owned(),
    //                   ]);
    //               */
    //   }

    //   #[tokio::test]
    //   async fn type_inconsistency_while_expanding_transform() {
    //       let config: ConfigBuilder = toml::from_str(indoc! {r#"
    //           [sources.input]
    //             type = "demo_logs"
    //             format = "shuffle"
    //             lines = ["one", "two"]
    //             count = 5

    //           [transforms.foo]
    //             inputs = ["input"]
    //             type = "compound"
    //             [[transforms.foo.steps]]
    //               id = "step1"
    //               type = "log_to_metric"
    //               [[transforms.foo.steps.metrics]]
    //                 type = "counter"
    //                 field = "c"
    //                 name = "sum"
    //                 namespace = "ns"
    //             [[transforms.foo.steps]]
    //               id = "step2"
    //               type = "log_to_metric"
    //               [[transforms.foo.steps.metrics]]
    //                 type = "counter"
    //                 field = "c"
    //                 name = "sum"
    //                 namespace = "ns"

    //           [sinks.output]
    //             type = "console"
    //             inputs = [ "foo.step2" ]
    //             encoding = "json"
    //             target = "stdout"
    //       "#})
    //       .unwrap();

    //       let err = crate::config::compiler::compile(config).err().unwrap();
    //       assert_eq!(
    //           err,
    //           vec!["Data type mismatch between foo.step1 (Metric) and foo.step2 (Log)".to_owned()]
    //       );
    //   }

    //   #[tokio::test]
    //   async fn invalid_name_in_expanded_transform() {
    //       let config: ConfigBuilder = toml::from_str(indoc! {r#"
    //           [sources.input]
    //             type = "demo_logs"
    //             format = "shuffle"
    //             lines = ["one", "two"]
    //             count = 5

    //           [transforms.foo]
    //             inputs = ["input"]
    //             type = "compound"
    //             [[transforms.foo.steps]]
    //               type = "log_to_metric"
    //               [[transforms.foo.steps.metrics]]
    //                 type = "counter"
    //                 field = "c"
    //                 name = "sum"
    //                 namespace = "ns"
    //             [[transforms.foo.steps]]
    //               id = "0"
    //               type = "log_to_metric"
    //               [[transforms.foo.steps.metrics]]
    //                 type = "counter"
    //                 field = "c"
    //                 name = "sum"
    //                 namespace = "ns"

    //           [sinks.output]
    //             type = "console"
    //             inputs = [ "foo.0" ]
    //             encoding = "json"
    //             target = "stdout"
    //       "#})
    //       .unwrap();

    //       let err = crate::config::compiler::compile(config).err().unwrap();
    //       assert_eq!(
    //           err,
    //           vec![
    //               "failed to expand transform 'foo': conflicting id found while expanding transform"
    //                   .to_owned()
    //           ]
    //       );
    //   }
}
