#[cfg(all(test, feature = "transforms-add_fields", feature = "transforms-route"))]
mod tests;
mod unit_test_components;

use crate::{
    conditions::Condition,
    config::{
        self, compiler::expand_macros, loading, ComponentKey, Config, ConfigBuilder, ConfigDiff,
        ConfigPath, SinkOuter, SourceOuter, TestDefinition, TestInput, TestInputValue, TestOutput,
    },
    event::{Event, Value},
    test_util::random_string,
    topology::{
        self,
        builder::{self, Pieces},
    },
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

use self::unit_test_components::{
    UnitTestSinkCheck, UnitTestSinkConfig, UnitTestSinkResult, UnitTestSourceConfig,
};

pub struct UnitTest {
    pub name: String,
    config: Config,
    diff: ConfigDiff,
    pieces: Pieces,
    test_result_rxs: Vec<Receiver<UnitTestSinkResult>>,
}

pub struct UnitTestResult {
    pub errors: Vec<String>,
}

impl UnitTest {
    pub async fn run(self) -> UnitTestResult {
        let (topology, _) = topology::start_validated(self.config, self.diff, self.pieces)
            .await
            .unwrap();
        let _ = topology.sources_finished().await;
        let _stop_complete = topology.stop();

        let mut in_flight = self
            .test_result_rxs
            .into_iter()
            .collect::<FuturesUnordered<_>>();

        let mut errors = Vec::new();
        while let Some(partial_result) = in_flight.next().await {
            let partial_result = partial_result.expect(
                "An unexpected error occurred while executing unit tests. Please try again.",
            );
            errors.extend(partial_result.test_errors);
        }

        UnitTestResult { errors }
    }
}

pub async fn build_unit_tests_main(paths: &[ConfigPath]) -> Result<Vec<UnitTest>, Vec<String>> {
    config::init_log_schema(paths, false)?;

    let (config_builder, _) = loading::load_builder_from_paths(paths)?;

    build_unit_tests(config_builder).await
}

async fn build_unit_tests(mut config_builder: ConfigBuilder) -> Result<Vec<UnitTest>, Vec<String>> {
    sanitize_config(&mut config_builder);
    let test_definitions = std::mem::take(&mut config_builder.tests);
    let mut tests = Vec::new();
    let mut build_errors = Vec::new();
    let metadata = UnitTestBuildMetadata::initialize(&mut config_builder)?;

    for mut test_definition in test_definitions {
        let test_name = test_definition.name.clone();
        // Move the legacy single test input into the inputs list if it exists
        let legacy_input = std::mem::take(&mut test_definition.input);
        if let Some(input) = legacy_input {
            test_definition.inputs.push(input);
        }
        // match build_unit_test(test_definition, config_builder.clone()).await {
        match build_unit_test(&metadata, test_definition, config_builder.clone()).await {
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
            .cloned()
            .collect::<HashSet<_>>();
        new_inputs.extend(route_inputs);
        transform.inputs = new_inputs.into_iter().collect();
    }
}

pub struct UnitTestBuildMetadata {
    pub available_insert_targets: HashSet<ComponentKey>,
    source_ids: HashMap<ComponentKey, String>,
    template_sources: IndexMap<ComponentKey, UnitTestSourceConfig>,
    sink_ids: HashMap<ComponentKey, String>,
}

impl UnitTestBuildMetadata {
    pub fn initialize(config_builder: &mut ConfigBuilder) -> Result<Self, Vec<String>> {
        // A unique id used to name test sources and sinks to avoid name clashes
        let random_id = random_string(15);

        let available_insert_targets = config_builder
            .transforms
            .keys()
            .cloned()
            .collect::<HashSet<_>>();

        // Mapping from transform name to unit test source name
        let source_ids = available_insert_targets
            .iter()
            .map(|key| (key.clone(), format!("{}-{}-{}", key, "source", random_id)))
            .collect::<HashMap<_, _>>();

        // Connect a test source to every transform
        let mut template_sources = IndexMap::new();
        for (key, transform) in config_builder.transforms.iter_mut() {
            let test_source_id = source_ids
                .get(key)
                .expect("Missing test source for a transform")
                .clone();
            transform.inputs.push(test_source_id);

            template_sources.insert(key.clone(), UnitTestSourceConfig::default());
        }

        // In order to attach a sink to every valid extraction point, we need to
        // expand any relevant transforms
        let mut builder = config_builder.clone();
        let _ = expand_macros(&mut builder)?;
        let available_extract_targets = builder
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
                    format!(
                        "{}-{}-{}",
                        key.to_string().replace(".", "-"),
                        "sink",
                        random_id
                    ),
                )
            })
            .collect::<HashMap<_, _>>();

        Ok(Self {
            available_insert_targets,
            source_ids,
            template_sources,
            sink_ids,
        })
    }

    pub fn hydrate_into_sources(
        &self,
        inputs: HashMap<ComponentKey, Vec<Event>>,
    ) -> IndexMap<ComponentKey, SourceOuter> {
        let mut template_sources = self.template_sources.clone();
        for (insert_at, events) in inputs {
            let source_config = template_sources.get_mut(&insert_at).unwrap_or_else(|| {
                panic!(
                    "Invalid input: cannot insert at {:?}",
                    insert_at.to_string()
                )
            });
            source_config.events.extend(events);
        }
        template_sources
            .into_iter()
            .map(|(transform_key, source_config)| {
                let source_key: &str = self
                    .source_ids
                    .get(&transform_key)
                    .expect("Corresponding source must exist")
                    .as_ref();
                (
                    ComponentKey::from(source_key),
                    SourceOuter::new(source_config),
                )
            })
            .collect::<IndexMap<_, _>>()
    }

    pub fn hydrate_into_sinks(
        &self,
        test_name: &str,
        outputs: IndexMap<ComponentKey, Vec<Vec<Box<dyn Condition>>>>,
        no_outputs_from: Vec<ComponentKey>,
    ) -> (
        Vec<Receiver<UnitTestSinkResult>>,
        IndexMap<ComponentKey, SinkOuter<String>>,
    ) {
        let mut test_result_rxs = Vec::new();
        // Connect a sink to every transform output
        let mut template_sinks = IndexMap::new();
        for (transform_id, _) in self.sink_ids.iter() {
            let (tx, rx) = oneshot::channel();
            let sink_config = UnitTestSinkConfig {
                test_name: test_name.to_string(),
                transform_id: transform_id.to_string(),
                result_tx: Arc::new(Mutex::new(Some(tx))),
                check: UnitTestSinkCheck::NoOp,
            };
            template_sinks.insert(transform_id.clone(), sink_config);
            test_result_rxs.push(rx);
        }

        // Add checks to sinks associated with an extract_from
        for (transform_id, checks) in outputs {
            let sink_config = template_sinks
                .get_mut(&transform_id)
                .expect("Sink does not exist");
            sink_config.check = UnitTestSinkCheck::Checks(checks);
        }

        // Add no outputs assertion to relevant sinks
        for transform_id in no_outputs_from {
            let sink_config = template_sinks
                .get_mut(&transform_id)
                .expect("Sink does not exist");
            sink_config.check = UnitTestSinkCheck::NoOutputs;
        }

        let sinks = template_sinks
            .into_iter()
            .map(|(transform_id, sink_config)| {
                let sink_id = self
                    .sink_ids
                    .get(&transform_id)
                    .expect("Sink does not exist")
                    .as_ref();
                (
                    ComponentKey::from(sink_id),
                    SinkOuter::new(vec![transform_id.to_string()], Box::new(sink_config)),
                )
            })
            .collect::<IndexMap<_, _>>();

        (test_result_rxs, sinks)
    }
}

async fn build_unit_test(
    metadata: &UnitTestBuildMetadata,
    test: TestDefinition,
    mut config_builder: ConfigBuilder,
) -> Result<UnitTest, Vec<String>> {
    let inputs = build_and_validate_inputs(&test.inputs, &metadata.available_insert_targets)?;
    let sources = metadata.hydrate_into_sources(inputs);

    if test.outputs.is_empty() && test.no_outputs_from.is_empty() {
        return Err(vec![
            "unit test must contain at least one of `outputs` or `no_outputs_from`.".to_string(),
        ]);
    }

    let outputs = build_outputs(&test.outputs)?;
    let (test_result_rxs, sinks) =
        metadata.hydrate_into_sinks(test.name.as_ref(), outputs, test.no_outputs_from.clone());

    config_builder.sources = sources;
    config_builder.sinks = sinks;
    let config = config_builder.build()?;
    let diff = config::ConfigDiff::initial(&config);
    let pieces = builder::build_pieces(&config, &diff, HashMap::new()).await?;

    Ok(UnitTest {
        name: test.name,
        config,
        diff,
        pieces,
        test_result_rxs,
    })
}

fn build_and_validate_inputs(
    test_inputs: &[TestInput],
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
            match build_input_event(input) {
                Ok(input_event) => {
                    inputs
                        .entry(input.insert_at.clone())
                        .and_modify(|events: &mut Vec<Event>| {
                            events.push(input_event.clone());
                        })
                        .or_insert_with(|| vec![input_event]);
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
    test_outputs: &[TestOutput],
) -> Result<IndexMap<ComponentKey, Vec<Vec<Box<dyn Condition>>>>, Vec<String>> {
    let mut outputs: IndexMap<ComponentKey, Vec<Vec<Box<dyn Condition>>>> = IndexMap::new();
    let mut errors = Vec::new();

    for output in test_outputs {
        let mut conditions = Vec::new();
        for (index, condition) in output
            .conditions
            .clone()
            .unwrap_or_default()
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
            .and_modify(|existing_conditions| existing_conditions.push(conditions.clone()))
            .or_insert(vec![conditions]);
    }

    if errors.is_empty() {
        Ok(outputs)
    } else {
        Err(errors)
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
