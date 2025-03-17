// should match vector-unit-test-tests feature
#[cfg(all(
    test,
    feature = "sources-demo_logs",
    feature = "transforms-remap",
    feature = "transforms-route",
    feature = "transforms-filter",
    feature = "transforms-reduce",
    feature = "sinks-console"
))]
mod tests;
mod unit_test_components;

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

use futures_util::{stream::FuturesUnordered, StreamExt};
use indexmap::IndexMap;
use tokio::sync::{
    oneshot::{self, Receiver},
    Mutex,
};
use uuid::Uuid;
use vrl::{
    compiler::{state::RuntimeState, Context, TargetValue, TimeZone},
    diagnostic::Formatter,
    value,
};

pub use self::unit_test_components::{
    UnitTestSinkCheck, UnitTestSinkConfig, UnitTestSinkResult, UnitTestSourceConfig,
    UnitTestStreamSinkConfig, UnitTestStreamSourceConfig,
};
use super::{compiler::expand_globs, graph::Graph, transform::get_transform_output_ids, OutputId};
use crate::{
    conditions::Condition,
    config::{
        self, loading, ComponentKey, Config, ConfigBuilder, ConfigPath, SinkOuter, SourceOuter,
        TestDefinition, TestInput, TestOutput,
    },
    event::{Event, EventMetadata, LogEvent},
    signal,
    topology::{builder::TopologyPieces, RunningTopology},
};

pub struct UnitTest {
    pub name: String,
    config: Config,
    pieces: TopologyPieces,
    test_result_rxs: Vec<Receiver<UnitTestSinkResult>>,
}

pub struct UnitTestResult {
    pub errors: Vec<String>,
}

impl UnitTest {
    pub async fn run(self) -> UnitTestResult {
        let diff = config::ConfigDiff::initial(&self.config);
        let (topology, _) = RunningTopology::start_validated(self.config, diff, self.pieces)
            .await
            .unwrap();
        topology.sources_finished().await;
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

/// Loads Log Schema from configurations and sets global schema.
/// Once this is done, configurations can be correctly loaded using
/// configured log schema defaults.
/// If deny is set, will panic if schema has already been set.
fn init_log_schema_from_paths(
    config_paths: &[ConfigPath],
    deny_if_set: bool,
) -> Result<(), Vec<String>> {
    let builder = config::loading::load_builder_from_paths(config_paths)?;
    vector_lib::config::init_log_schema(builder.global.log_schema, deny_if_set);
    Ok(())
}

pub async fn build_unit_tests_main(
    paths: &[ConfigPath],
    signal_handler: &mut signal::SignalHandler,
) -> Result<Vec<UnitTest>, Vec<String>> {
    init_log_schema_from_paths(paths, false)?;
    let mut secrets_backends_loader = loading::load_secret_backends_from_paths(paths)?;
    let config_builder = if secrets_backends_loader.has_secrets_to_retrieve() {
        let resolved_secrets = secrets_backends_loader
            .retrieve(&mut signal_handler.subscribe())
            .await
            .map_err(|e| vec![e])?;
        loading::load_builder_from_paths_with_secrets(paths, resolved_secrets)?
    } else {
        loading::load_builder_from_paths(paths)?
    };

    build_unit_tests(config_builder).await
}

pub async fn build_unit_tests(
    mut config_builder: ConfigBuilder,
) -> Result<Vec<UnitTest>, Vec<String>> {
    // Sanitize config by removing existing sources and sinks
    config_builder.sources = Default::default();
    config_builder.sinks = Default::default();

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
        match build_unit_test(&metadata, test_definition, config_builder.clone()).await {
            Ok(test) => tests.push(test),
            Err(errors) => {
                let mut test_error = errors.join("\n");
                // Indent all line breaks
                test_error = test_error.replace('\n', "\n  ");
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

pub struct UnitTestBuildMetadata {
    // A set of all valid insert_at targets, used to validate test inputs.
    available_insert_targets: HashSet<ComponentKey>,
    // A mapping from transform name to unit test source name.
    source_ids: HashMap<ComponentKey, String>,
    // A base setup of all necessary unit test sources that can be "hydrated"
    // with test input events to produces sources used in a particular test.
    template_sources: IndexMap<ComponentKey, UnitTestSourceConfig>,
    // A mapping from transform name to unit test sink name.
    sink_ids: HashMap<OutputId, String>,
}

impl UnitTestBuildMetadata {
    pub fn initialize(config_builder: &mut ConfigBuilder) -> Result<Self, Vec<String>> {
        // A unique id used to name test sources and sinks to avoid name clashes
        let random_id = Uuid::new_v4().to_string();

        let available_insert_targets = config_builder
            .transforms
            .keys()
            .cloned()
            .collect::<HashSet<_>>();

        let source_ids = available_insert_targets
            .iter()
            .map(|key| (key.clone(), format!("{}-{}-{}", key, "source", random_id)))
            .collect::<HashMap<_, _>>();

        // Map a test source to every transform
        let mut template_sources = IndexMap::new();
        for (key, transform) in config_builder.transforms.iter_mut() {
            let test_source_id = source_ids
                .get(key)
                .expect("Missing test source for a transform")
                .clone();
            transform.inputs.extend(Some(test_source_id));

            template_sources.insert(key.clone(), UnitTestSourceConfig::default());
        }

        let builder = config_builder.clone();
        let available_extract_targets = builder
            .transforms
            .iter()
            .flat_map(|(key, transform)| {
                get_transform_output_ids(
                    transform.inner.as_ref(),
                    key.clone(),
                    builder.schema.log_namespace(),
                )
            })
            .collect::<HashSet<_>>();

        let sink_ids = available_extract_targets
            .iter()
            .map(|key| {
                (
                    key.clone(),
                    format!(
                        "{}-{}-{}",
                        key.to_string().replace('.', "-"),
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

    /// Convert test inputs into sources for use in a unit testing topology
    pub fn hydrate_into_sources(
        &self,
        inputs: &[TestInput],
    ) -> Result<IndexMap<ComponentKey, SourceOuter>, Vec<String>> {
        let inputs = build_and_validate_inputs(inputs, &self.available_insert_targets)?;
        let mut template_sources = self.template_sources.clone();
        Ok(inputs
            .into_iter()
            .map(|(insert_at, events)| {
                let mut source_config =
                    template_sources
                        .shift_remove(&insert_at)
                        .unwrap_or_else(|| {
                            // At this point, all inputs should have been validated to
                            // correspond with valid transforms, and all valid transforms
                            // have a source attached.
                            panic!(
                                "Invalid input: cannot insert at {:?}",
                                insert_at.to_string()
                            )
                        });
                source_config.events.extend(events);
                let id: &str = self
                    .source_ids
                    .get(&insert_at)
                    .expect("Corresponding source must exist")
                    .as_ref();
                (ComponentKey::from(id), SourceOuter::new(source_config))
            })
            .collect::<IndexMap<_, _>>())
    }

    /// Convert test outputs into sinks for use in a unit testing topology
    pub fn hydrate_into_sinks(
        &self,
        test_name: &str,
        outputs: &[TestOutput],
        no_outputs_from: &[OutputId],
    ) -> Result<
        (
            Vec<Receiver<UnitTestSinkResult>>,
            IndexMap<ComponentKey, SinkOuter<String>>,
        ),
        Vec<String>,
    > {
        if outputs.is_empty() && no_outputs_from.is_empty() {
            return Err(vec![
                "unit test must contain at least one of `outputs` or `no_outputs_from`."
                    .to_string(),
            ]);
        }
        let outputs = build_outputs(outputs)?;

        let mut template_sinks = IndexMap::new();
        let mut test_result_rxs = Vec::new();
        // Add sinks with checks
        for (ids, checks) in outputs {
            let (tx, rx) = oneshot::channel();
            let sink_ids = ids.clone();
            let sink_config = UnitTestSinkConfig {
                test_name: test_name.to_string(),
                transform_ids: ids.iter().map(|id| id.to_string()).collect(),
                result_tx: Arc::new(Mutex::new(Some(tx))),
                check: UnitTestSinkCheck::Checks(checks),
            };

            test_result_rxs.push(rx);
            template_sinks.insert(sink_ids, sink_config);
        }

        // Add sinks with no outputs check
        for id in no_outputs_from {
            let (tx, rx) = oneshot::channel();
            let sink_config = UnitTestSinkConfig {
                test_name: test_name.to_string(),
                transform_ids: vec![id.to_string()],
                result_tx: Arc::new(Mutex::new(Some(tx))),
                check: UnitTestSinkCheck::NoOutputs,
            };

            test_result_rxs.push(rx);
            template_sinks.insert(vec![id.clone()], sink_config);
        }

        let sinks = template_sinks
            .into_iter()
            .map(|(transform_ids, sink_config)| {
                let transform_ids_str = transform_ids
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>();
                let sink_ids = transform_ids
                    .iter()
                    .map(|transform_id| {
                        self.sink_ids
                            .get(transform_id)
                            .expect("Sink does not exist")
                            .as_str()
                    })
                    .collect::<Vec<_>>();
                let sink_id = sink_ids.join(",");
                (
                    ComponentKey::from(sink_id),
                    SinkOuter::new(transform_ids_str, sink_config),
                )
            })
            .collect::<IndexMap<_, _>>();

        Ok((test_result_rxs, sinks))
    }
}

// Find all components that participate in the test
fn get_relevant_test_components(
    sources: &[&ComponentKey],
    graph: &Graph,
) -> Result<HashSet<String>, Vec<String>> {
    graph.check_for_cycles().map_err(|error| vec![error])?;
    let mut errors = Vec::new();
    let mut components = HashSet::new();
    for source in sources {
        let paths = graph.paths_to_sink_from(source);
        if paths.is_empty() {
            errors.push(format!(
                "Unable to complete topology between input target '{}' and output target(s)",
                source
                    .to_string()
                    .rsplit_once("-source-")
                    .unwrap_or(("", ""))
                    .0
            ));
        } else {
            for path in paths {
                components.extend(path.into_iter().map(|key| key.to_string()));
            }
        }
    }

    if errors.is_empty() {
        Ok(components)
    } else {
        Err(errors)
    }
}

async fn build_unit_test(
    metadata: &UnitTestBuildMetadata,
    test: TestDefinition<String>,
    mut config_builder: ConfigBuilder,
) -> Result<UnitTest, Vec<String>> {
    let transform_only_config = config_builder.clone();
    let transform_only_graph = Graph::new_unchecked(
        &transform_only_config.sources,
        &transform_only_config.transforms,
        &transform_only_config.sinks,
        transform_only_config.schema,
    );
    let test = test.resolve_outputs(&transform_only_graph)?;

    let sources = metadata.hydrate_into_sources(&test.inputs)?;
    let (test_result_rxs, sinks) =
        metadata.hydrate_into_sinks(&test.name, &test.outputs, &test.no_outputs_from)?;

    config_builder.sources = sources;
    config_builder.sinks = sinks;
    expand_globs(&mut config_builder);

    let graph = Graph::new_unchecked(
        &config_builder.sources,
        &config_builder.transforms,
        &config_builder.sinks,
        config_builder.schema,
    );

    let mut valid_components = get_relevant_test_components(
        config_builder.sources.keys().collect::<Vec<_>>().as_ref(),
        &graph,
    )?;

    // Preserve the original unexpanded transform(s) which are valid test insertion points
    let unexpanded_transforms = valid_components
        .iter()
        .filter_map(|component| {
            component
                .split_once('.')
                .map(|(original_name, _)| original_name.to_string())
        })
        .collect::<Vec<_>>();
    valid_components.extend(unexpanded_transforms);

    // Remove all transforms that are not relevant to the current test
    config_builder.transforms = config_builder
        .transforms
        .into_iter()
        .filter(|(key, _)| valid_components.contains(&key.to_string()))
        .collect();

    // Sanitize the inputs of all relevant transforms
    let graph = Graph::new_unchecked(
        &config_builder.sources,
        &config_builder.transforms,
        &config_builder.sinks,
        config_builder.schema,
    );
    let valid_inputs = graph.input_map()?;
    for (_, transform) in config_builder.transforms.iter_mut() {
        let inputs = std::mem::take(&mut transform.inputs);
        transform.inputs = inputs
            .into_iter()
            .filter(|input| valid_inputs.contains_key(input))
            .collect();
    }

    if let Some(sink) = get_loose_end_outputs_sink(&config_builder) {
        config_builder
            .sinks
            .insert(ComponentKey::from(Uuid::new_v4().to_string()), sink);
    }
    let config = config_builder.build()?;
    let diff = config::ConfigDiff::initial(&config);
    let pieces = TopologyPieces::build(&config, &diff, HashMap::new(), Default::default()).await?;

    Ok(UnitTest {
        name: test.name,
        config,
        pieces,
        test_result_rxs,
    })
}

/// Near the end of building a unit test, it's possible that we've included a
/// transform(s) with multiple outputs where at least one of its output is
/// consumed but its other outputs are left unconsumed.
///
/// To avoid warning logs that occur when building such topologies, we construct
/// a NoOp sink here whose sole purpose is to consume any "loose end" outputs.
fn get_loose_end_outputs_sink(config: &ConfigBuilder) -> Option<SinkOuter<String>> {
    let config = config.clone();
    let transform_ids = config.transforms.iter().flat_map(|(key, transform)| {
        get_transform_output_ids(
            transform.inner.as_ref(),
            key.clone(),
            config.schema.log_namespace(),
        )
        .map(|output| output.to_string())
        .collect::<Vec<_>>()
    });

    let mut loose_end_outputs = Vec::new();
    for id in transform_ids {
        if !config
            .transforms
            .iter()
            .any(|(_, transform)| transform.inputs.contains(&id))
            && !config
                .sinks
                .iter()
                .any(|(_, sink)| sink.inputs.contains(&id))
        {
            loose_end_outputs.push(id);
        }
    }

    if loose_end_outputs.is_empty() {
        None
    } else {
        let noop_sink = UnitTestSinkConfig {
            test_name: "".to_string(),
            transform_ids: vec![],
            result_tx: Arc::new(Mutex::new(None)),
            check: UnitTestSinkCheck::NoOp,
        };
        Some(SinkOuter::new(loose_end_outputs, noop_sink))
    }
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
) -> Result<IndexMap<Vec<OutputId>, Vec<Vec<Condition>>>, Vec<String>> {
    let mut outputs: IndexMap<Vec<OutputId>, Vec<Vec<Condition>>> = IndexMap::new();
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
            .entry(output.extract_from.clone().to_vec())
            .and_modify(|existing_conditions| existing_conditions.push(conditions.clone()))
            .or_insert(vec![conditions.clone()]);
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
            Some(v) => Ok(Event::Log(LogEvent::from_str_legacy(v.clone()))),
            None => Err("input type 'raw' requires the field 'value'".to_string()),
        },
        "vrl" => {
            if let Some(source) = &input.source {
                let fns = vrl::stdlib::all();
                let result = vrl::compiler::compile(source, &fns)
                    .map_err(|e| Formatter::new(source, e.clone()).to_string())?;

                let mut target = TargetValue {
                    value: value!({}),
                    metadata: value::Value::Object(BTreeMap::new()),
                    secrets: value::Secrets::default(),
                };

                let mut state = RuntimeState::default();
                let timezone = TimeZone::default();
                let mut ctx = Context::new(&mut target, &mut state, &timezone);

                result
                    .program
                    .resolve(&mut ctx)
                    .map(|_| {
                        Event::Log(LogEvent::from_parts(
                            target.value.clone(),
                            EventMetadata::default_with_value(target.metadata.clone()),
                        ))
                    })
                    .map_err(|e| e.to_string())
            } else {
                Err("input type 'vrl' requires the field 'source'".to_string())
            }
        }
        "log" => {
            if let Some(log_fields) = &input.log_fields {
                let mut event = LogEvent::from_str_legacy("");
                for (path, value) in log_fields {
                    event
                        .parse_path_and_insert(path, value.clone())
                        .map_err(|e| e.to_string())?;
                }
                Ok(event.into())
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
