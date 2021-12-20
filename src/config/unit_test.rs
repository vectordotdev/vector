use std::collections::HashMap;

use indexmap::IndexMap;

use super::{
    graph::Graph, ComponentKey, Config, ConfigBuilder, ConfigDiff, ConfigPath, GlobalOptions,
    TestDefinition, TestInput, TestInputValue, TransformConfig, TransformContext, TestOutput, unit_test_v2::UnitTestSinkResult,
};
use crate::{
    conditions::Condition,
    config::{
        self,
        unit_test_v2::{UnitTestSinkConfig, UnitTestSourceConfig, UnitTestSinkCheck},
        OutputId, SinkOuter, SourceOuter,
    },
    event::{Event, Value},
    topology::{self, builder::{Pieces, self}},
};
use futures_util::{stream::FuturesUnordered, StreamExt};
use indexmap::IndexMap;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{Mutex, oneshot::{Receiver, self}};
use tokio_stream::wrappers::ReceiverStream;

// async fn build_unit_tests(mut builder: ConfigBuilder) -> Result<Vec<UnitTest>, Vec<String>> {
//     let mut tests = vec![];
//     let mut errors = vec![];

//     let expansions = super::compiler::expand_macros(&mut builder)?;

//     // Resolve inputs via the graph, even though we haven't fully validated everything here
//     let graph = Graph::new_unchecked(&IndexMap::new(), &builder.transforms, &builder.sinks);
//     let transforms = std::mem::take(&mut builder.transforms)
//         .into_iter()
//         .map(|(key, transform)| {
//             let inputs = graph.inputs_for(&key);
//             (key, transform.with_inputs(inputs))
//         })
//         .collect();
//     let sinks = std::mem::take(&mut builder.sinks)
//         .into_iter()
//         .map(|(key, sink)| {
//             let inputs = graph.inputs_for(&key);
//             (key, sink.with_inputs(inputs))
//         })
//         .collect();
//     println!("test definitions {:?}", builder.tests);
//     println!("transforms: {:?}", transforms);
//     println!("sinks: {:?}", sinks);

//     // Don't let this escape since it's not validated
//     let config = Config {
//         global: builder.global,
//         #[cfg(feature = "api")]
//         api: builder.api,
//         #[cfg(feature = "datadog-pipelines")]
//         datadog: builder.datadog,
//         healthchecks: builder.healthchecks,
//         enrichment_tables: builder.enrichment_tables,
//         sources: builder.sources,
//         sinks,
//         transforms,
//         tests: builder.tests,
//         expansions,
//         ..Config::default()
//     };

//     for test in &config.tests {
//         match build_unit_test(test, &config).await {
//             Ok(t) => tests.push(t),
//             Err(errs) => {
//                 let mut test_err = errs.join("\n");
//                 // Indent all line breaks
//                 test_err = test_err.replace("\n", "\n  ");
//                 test_err.insert_str(0, &format!("Failed to build test '{}':\n  ", test.name));
//                 errors.push(test_err);
//             }
//         }
//     }

//     if errors.is_empty() {
//         Ok(tests)
//     } else {
//         Err(errors)
//     }
// }

pub struct UnitTest {
    pub name: String,
    config: Config,
    diff: ConfigDiff, 
    pieces: Pieces,
    sink_rxs: Vec<Receiver<UnitTestSinkResult>>,
    // globals: GlobalOptions,
}

pub struct UnitTestResult {
    pub name: String,
    pub test_errors: Vec<String>,
}

pub async fn build_unit_tests_main(paths: &[ConfigPath]) -> Result<Vec<UnitTest>, Vec<String>> {
    config::init_log_schema(paths, false)?;

    let (mut config_builder, _) = super::loading::load_builder_from_paths(paths)?;
    let tests = std::mem::take(&mut config_builder.tests);
    let mut unit_tests = Vec::new();
    let mut build_errors = Vec::new();
    // todo: generate random id for inserted components

    for test in tests {
        let test_name = test.name.clone();
        match build_unit_test(test, config_builder.clone()).await {
            Ok(test) => unit_tests.push(test),
            Err(errors) => {
                let mut test_error = errors.join("\n");
                // Indent all line breaks
                test_error = test_error.replace("\n", "\n  ");
                test_error.insert_str(0, &format!("Failed to build test '{}':\n  ", test_name));
                build_errors.push(test_error);
            },
        }
    }

    Ok(unit_tests)
}

async fn build_unit_test(
    test: TestDefinition,
    mut config_builder: ConfigBuilder,
) -> Result<UnitTest, Vec<String>> {
    let mut build_errors = Vec::new();

    // Rid the transform inputs of any existing sources
    let graph = Graph::new_unchecked(&IndexMap::new(), &config_builder.transforms, &IndexMap::new());
    for (key, transform) in config_builder.transforms.iter_mut() {
        transform.inputs = graph.inputs_for(key).into_iter().map(|id| id.to_string()).collect::<Vec<_>>();
    }

    println!("test has the following inputs: {:?}\n", test.inputs);
    let inputs = build_inputs(&test.inputs).unwrap_or_else(|errors| {
      build_errors.extend(errors);
      Vec::new()
    });

    // Connect a test source to each unique insert_at target transform
    let mut test_sources = IndexMap::new();
    for (target_transform_id, event) in inputs {
        let test_source_id = format!("{}-{}", target_transform_id, "unit-test-source");

        if test_sources.get(&test_source_id).is_none() {
            test_sources.insert(test_source_id.clone(), Vec::new());
            match config_builder.transforms.get_mut(&target_transform_id) {
                Some(transform) => transform.inputs.push(test_source_id.clone()),
                None => build_errors.push("No transform found for that insert_at".to_string()),
            }
        }
        test_sources
            .entry(test_source_id)
            .and_modify(|events| events.push(event));
    }

    let sources = build_unit_test_sources(test_sources);
    println!("test created the following sources: {:?}\n", sources);

    if test.outputs.is_empty() && test.no_outputs_from.is_empty() {
        build_errors.push("unit test must contain at least one of `outputs` or `no_outputs_from`.".to_string());
        return Err(build_errors);
    }

    let outputs = build_outputs(&test.outputs).unwrap_or_else(|errors| {
      build_errors.extend(errors);
      Vec::new()
    });
    println!("built output errors: {:?}", build_errors); 
    // Connect a test sink to each unique extract_from target transform
    let mut test_output_sinks = IndexMap::new();
    for (target_transform_id, conditions) in outputs {
        // map of extract_from string --> conditions
        let test_sink_id = format!("{}-{}", target_transform_id.to_string().replace(".", "-"), "unit-test-sink");
        test_output_sinks.entry(test_sink_id).and_modify(|(_, sink_conditions): &mut (String, Vec<Vec<Box<dyn Condition>>>)| sink_conditions.push(conditions.clone())).or_insert((target_transform_id.to_string(), vec![conditions]));
    }

    // Connect a test sink to each unique no_outputs_from target transform
    let mut test_non_output_sinks = test.no_outputs_from.into_iter().map(|target_transform_id| {
        let test_sink_id = format!("{}-{}", target_transform_id.to_string().replace(".", "-"), "unit-test-sink");
        (test_sink_id, target_transform_id.to_string())
    }).collect::<IndexMap<_, _>>();

    let mut sinks = IndexMap::new();
    // let mut sink_rxs = IndexMap::new();
    let mut sink_rxs = Vec::new();
    for (key, (input, checks)) in test_output_sinks {
        let key = ComponentKey::from(key);
        let (tx, rx) = oneshot::channel();
        let sink_config = UnitTestSinkConfig {
            name: test.name.clone(),
            result_tx: Arc::new(Mutex::new(Some(tx))),
            check: UnitTestSinkCheck::Checks(checks),
        };
        // sink_rxs.insert(key.clone(), rx);
        sink_rxs.push(rx);
        sinks.insert(key, SinkOuter::new(vec![input], Box::new(sink_config)));
    }

    for (key, input) in test_non_output_sinks {
        let key = ComponentKey::from(key);
        let (tx, rx) = oneshot::channel();
        let sink_config = UnitTestSinkConfig {
            name: test.name.clone(),
            result_tx: Arc::new(Mutex::new(Some(tx))),
            check: UnitTestSinkCheck::NoOutputs,
        };
        // sink_rxs.insert(key.clone(), rx);
        sink_rxs.push(rx);
        sinks.insert(key, SinkOuter::new(vec![input], Box::new(sink_config)));
    }

    println!("test created the following sinks: {:?}\n", sinks);

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
        sink_rxs,
    })

}

fn build_inputs(test_inputs: &Vec<TestInput>) -> Result<Vec<(ComponentKey, Event)>, Vec<String>> {
    let mut inputs = Vec::new();
    let mut errors = Vec::new();
    if test_inputs.is_empty() {
        errors.push("must specify at least one input.".to_string());
        return Err(errors);
    }

    for input in test_inputs {
        match build_input_event(&input) {
            Ok(input_event) => inputs.push((input.insert_at.clone(), input_event)),
            Err(error) => errors.push(error),
        }
    }

    if errors.is_empty() {
        Ok(inputs)
    } else {
        Err(errors)
    }
}

fn build_outputs(test_outputs: &Vec<TestOutput>) -> Result<Vec<(ComponentKey, Vec<Box<dyn Condition>>)>, Vec<String>> {
    let mut outputs = Vec::new();
    let mut errors = Vec::new();


    for output in test_outputs {
        let mut conditions = Vec::new();
        // todo: are you allowed to have an output with no condition assigned? if not, raise an error
        for (index, condition) in output.conditions.clone().unwrap_or(Vec::new()).iter().enumerate() {
            match condition.build(&Default::default()) {
                Ok(condition) => conditions.push(condition),
                Err(error) => errors.push(format!(
                    "failed to create test condition '{}': {}",
                    index, error
                )),
            }
        }
        outputs.push((output.extract_from.clone(), conditions));
    }

    if errors.is_empty() {
        Ok(outputs)
    } else {
        Err(errors)
    }
}

fn build_unit_test_sources(
    sources: IndexMap<String, Vec<Event>>,
) -> IndexMap<ComponentKey, SourceOuter> {
    sources
        .into_iter()
        .map(|(key, events)| {
            let source_config = UnitTestSourceConfig {
                events,
                ..Default::default()
            };

            (key.into(), SourceOuter::new(source_config))
        })
        .collect::<IndexMap<_, _>>()
}

impl UnitTest {
    pub async fn run(self) -> (Vec<String>, Vec<String>) {
        let (topology, _) = topology::start_validated(self.config, self.diff, self.pieces).await.unwrap();
        let _ = topology.sources_finished().await;
        let _stop_complete = topology.stop();

        let mut in_flight = self.sink_rxs
            .into_iter()
            .collect::<FuturesUnordered<_>>();

        let mut inspections = Vec::new();
        let mut errors = Vec::new();
        while let Some(partial_result) = in_flight.next().await {
            let partial_result = partial_result.unwrap();
            if !partial_result.test_errors.is_empty() {
                errors.extend(partial_result.test_errors);
            }
        }

        (inspections, errors)
    }
}

pub struct UnitTestCheck {
    extract_from: ComponentKey,
    conditions: Vec<Box<dyn Condition>>,
}

fn event_to_string(event: &Event) -> String {
    match event {
        Event::Log(log) => serde_json::to_string(&log).unwrap_or_else(|_| "{}".into()),
        Event::Metric(metric) => serde_json::to_string(&metric).unwrap_or_else(|_| "{}".into()),
    }
}

fn events_to_string(name: &str, events: &[Event]) -> String {
    if events.len() > 1 {
        format!(
            "  {}s:\n    {}",
            name,
            events
                .iter()
                .map(event_to_string)
                .collect::<Vec<_>>()
                .join("\n    ")
        )
    } else {
        events
            .first()
            .map(|e| format!("  {}: {}", name, event_to_string(e)))
            .unwrap_or(format!("  no {}", name))
    }
}

// impl UnitTest {
//     // Executes each test and provides a tuple of inspections and error lists.
//     pub fn run(&mut self) -> (Vec<String>, Vec<String>) {
//         println!("Running a unit test! {:?}\n", self.name);
//         let mut errors = Vec::new();
//         let mut inspections = Vec::new();
//         let mut results = HashMap::new();

//         let mut inputs_by_target = HashMap::new();
//         for (targets, event) in &self.inputs {
//             for target in targets {
//                 let entry = inputs_by_target
//                     .entry(target.clone())
//                     .or_insert_with(Vec::new);
//                 entry.push(event.clone());
//             }
//         }

//         for (target, inputs) in inputs_by_target {
//             walk(
//                 &target,
//                 inputs,
//                 &mut self.transforms,
//                 &mut results,
//                 &self.globals,
//             );
//         }

//         for check in &self.checks {
//             if let Some((inputs, outputs)) = results.get(&check.extract_from) {
//                 if check.conditions.is_empty() {
//                     inspections.push(format!(
//                         "check transform '{}' payloads (events encoded as JSON):\n{}\n{}",
//                         check.extract_from,
//                         events_to_string(" input", inputs),
//                         events_to_string("output", outputs),
//                     ));
//                     continue;
//                 }
//                 let failed_conditions = check
//                     .conditions
//                     .iter()
//                     .enumerate()
//                     .flat_map(|(i, cond)| {
//                         let cond_errs = outputs
//                             .iter()
//                             .enumerate()
//                             .filter_map(|(j, e)| {
//                                 cond.check_with_context(e).err().map(|err| {
//                                     if outputs.len() > 1 {
//                                         format!("condition[{}], payload[{}]: {}", i, j, err)
//                                     } else {
//                                         format!("condition[{}]: {}", i, err)
//                                     }
//                                 })
//                             })
//                             .collect::<Vec<_>>();
//                         if cond_errs.len() < outputs.len() {
//                             // At least one output succeeded for this condition.
//                             Vec::new()
//                         } else {
//                             cond_errs
//                         }
//                     })
//                     .collect::<Vec<_>>();
//                 if !failed_conditions.is_empty() {
//                     errors.push(format!(
//                         "check transform '{}' failed conditions:\n  {}\npayloads (events encoded as JSON):\n{}\n{}",
//                         check.extract_from,
//                         failed_conditions.join("\n  "),
//                         events_to_string(" input", inputs),
//                         events_to_string("output", outputs),
//                     ));
//                 }
//                 if outputs.is_empty() {
//                     errors.push(format!(
//                         "check transform {:?} failed, no events received.",
//                         check.extract_from,
//                     ));
//                 }
//             } else {
//                 errors.push(format!(
//                     "check transform '{}' failed: received zero resulting events.",
//                     check.extract_from,
//                 ));
//             }
//         }

//         for tform in &self.no_outputs_from {
//             if let Some((inputs, outputs)) = results.get(tform) {
//                 if !outputs.is_empty() {
//                     errors.push(format!(
//                         "check transform '{}' failed: expected no outputs.\npayloads (events encoded as JSON):\n{}\n{}",
//                         tform,
//                         events_to_string(" input", inputs),
//                         events_to_string("output", outputs),
//                     ));
//                 }
//             }
//         }

//         (inspections, errors)
//     }
// }

//------------------------------------------------------------------------------


// fn build_inputs(
//     config: &Config,
//     definition: &TestDefinition,
// ) -> Result<Vec<(Vec<ComponentKey>, Event)>, Vec<String>> {
//     let mut inputs = Vec::new();
//     let mut errors = vec![];

//     if let Some(input_def) = &definition.input {
//         match build_input(config, input_def) {
//             Ok(input_event) => inputs.push(input_event),
//             Err(err) => errors.push(err),
//         }
//     } else if definition.inputs.is_empty() {
//         errors.push("must specify at least one input.".to_owned());
//     }
//     for input_def in &definition.inputs {
//         match build_input(config, input_def) {
//             Ok(input_event) => inputs.push(input_event),
//             Err(err) => errors.push(err),
//         }
//     }

//     if errors.is_empty() {
//         Ok(inputs)
//     } else {
//         Err(errors)
//     }
// }

// async fn build_unit_test(
//     definition: &TestDefinition,
//     config: &Config,
// ) -> Result<UnitTest, Vec<String>> {
//     let mut errors = vec![];

//     let inputs = match build_inputs(config, definition) {
//         Ok(inputs) => inputs,
//         Err(mut errs) => {
//             errors.append(&mut errs);
//             Vec::new()
//         }
//     };
//     println!("----- Building TestDef -------");
//     println!("test definition: {:?}", definition);
//     println!("-----");
//     println!("inputs: {:?}", inputs);
//     println!("-----");

//     // Maps transform names with their output targets (transforms that use it as
//     // an input).
//     let mut transform_outputs: IndexMap<ComponentKey, IndexMap<ComponentKey, ()>> = config
//         .transforms
//         .iter()
//         .map(|(k, _)| (k.clone(), IndexMap::new()))
//         .collect();

//     config.transforms.iter().for_each(|(k, t)| {
//         t.inputs.iter().for_each(|i| {
//             // TODO: this is intentionally ignoring named outputs for now
//             if let Some(outputs) = transform_outputs.get_mut(&i.component) {
//                 outputs.insert(k.clone(), ());
//             }
//         })
//     });
//     println!(
//         "a mapping between a transform --> its outputs:\n {:?}\n",
//         transform_outputs
//     );

//     for (i, (input_target, _)) in inputs.iter().enumerate() {
//         for target in input_target {
//             if !transform_outputs.contains_key(target) {
//                 errors.push(format!(
//                     "inputs[{}]: unable to locate target transform '{}'",
//                     i, target
//                 ));
//             }
//         }
//     }
//     if !errors.is_empty() {
//         return Err(errors);
//     }

//     let mut leaves: IndexMap<ComponentKey, ()> = IndexMap::new();
//     definition.outputs.iter().for_each(|o| {
//         leaves.insert(o.extract_from.clone(), ());
//     });
//     definition.no_outputs_from.iter().for_each(|o| {
//         leaves.insert(o.clone(), ());
//     });
//     println!(
//         "all components that we want to extract events from:\n {:?}\n",
//         leaves
//     );

//     // Reduce the configured transforms into just the ones connecting our test
//     // target with output targets.
//     reduce_transforms(
//         inputs
//             .iter()
//             .map(|(names, _)| names)
//             .flatten()
//             .cloned()
//             .collect::<Vec<_>>(),
//         &leaves,
//         &mut transform_outputs,
//     );

//     println!("the reduced version of the mapping between transform --> its outputs (we cut all the transforms not under test): \n {:?}\n", transform_outputs);

//     let diff = ConfigDiff::initial(config);
//     let (enrichment_tables, tables_errors) = load_enrichment_tables(config, &diff).await;

//     errors.extend(tables_errors);

//     // Build reduced transforms.
//     let mut transforms: IndexMap<ComponentKey, UnitTestTransform> = IndexMap::new();
//     for (id, transform_config) in &config.transforms {
//         if let Some(outputs) = transform_outputs.remove(id) {
//             let context = TransformContext {
//                 key: Some(id.clone()),
//                 globals: config.global.clone(),
//                 enrichment_tables: enrichment_tables.clone(),
//             };
//             println!("--- transform from config {:?}", transform_config);

//             match transform_config.inner.build(&context).await {
//                 Ok(transform) => {
//                     transforms.insert(
//                         id.clone(),
//                         UnitTestTransform {
//                             transform,
//                             config: transform_config.inner.clone(),
//                             next: outputs.into_iter().map(|(k, _)| k).collect(),
//                         },
//                     );
//                 }
//                 Err(err) => {
//                     errors.push(format!("failed to build transform '{}': {:#}", id, err));
//                 }
//             }
//         }
//     }

//     println!(
//         "we've built actual transforms for the following: \n {:?}\n",
//         transforms.keys()
//     );

//     if !errors.is_empty() {
//         return Err(errors);
//     }

//     println!("checking that every test output extraction matches a built transform\n");
//     definition.outputs.iter().for_each(|o| {
//         if !transforms.contains_key(&o.extract_from) {
//             let targets = inputs.iter().map(|(i, _)| i).flatten().collect::<Vec<_>>();
//             if targets.len() == 1 {
//                 errors.push(format!(
//                     "unable to complete topology between target transform '{}' and output target '{}'",
//                     targets.first().unwrap(), o.extract_from
//                 ));
//             } else {
//                 errors.push(format!(
//                     "unable to complete topology between target transforms {:?} and output target '{}'",
//                     targets.iter().map(|item| item.to_string()).collect::<Vec<_>>(), o.extract_from
//                 ));
//             }
//         }
//     });

//     // Build all output conditions.
//     let checks = definition
//         .outputs
//         .iter()
//         .map(|o| {
//             let mut conditions: Vec<Box<dyn Condition>> = Vec::new();
//             for (index, cond_conf) in o
//                 .conditions
//                 .as_ref()
//                 .unwrap_or(&Vec::new())
//                 .iter()
//                 .enumerate()
//             {
//                 println!("building a condition {:?}", cond_conf);
//                 match cond_conf.build(&Default::default()) {
//                     Ok(c) => conditions.push(c),
//                     Err(e) => errors.push(format!(
//                         "failed to create test condition '{}': {}",
//                         index, e,
//                     )),
//                 }
//             }

//             UnitTestCheck {
//                 extract_from: o.extract_from.clone(),
//                 conditions,
//             }
//         })
//         .collect();

//     println!("done building checks\n");

//     if definition.outputs.is_empty() && definition.no_outputs_from.is_empty() {
//         errors.push(
//             "unit test must contain at least one of `outputs` or `no_outputs_from`.".to_owned(),
//         );
//     }

//     enrichment_tables.finish_load();

//     if !errors.is_empty() {
//         Err(errors)
//     } else {
//         Ok(UnitTest {
//             name: definition.name.clone(),
//             inputs,
//             transforms,
//             checks,
//             no_outputs_from: definition.no_outputs_from.clone(),
//             globals: config.global.clone(),
//         })
//     }
// }

// -----

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

async fn run_tests(paths: &[ConfigPath]) -> Result<Vec<UnitTestResult>, Vec<String>> {
    // Load a ConfigBuilder from the user provided config paths
    let (mut builder, _) = super::loading::load_builder_from_paths(paths)?;

    let mut tests = std::mem::take(&mut builder.tests);

    let mut test_results = Vec::new();
    // todo: more efficient iteration for each test.
    // Can we avoid re-reading the config every time?
    // Can we reuse the same topology? We really only need to change the inputs each time...
    for test in tests {
        // Reload the ConfigBuilder from the user provided config paths
        let (mut builder, _) = super::loading::load_builder_from_paths(paths)?;

        let mut source_keys = Vec::new();
        let graph = Graph::new_unchecked(&IndexMap::new(), &builder.transforms, &IndexMap::new());
        let transforms = std::mem::take(&mut builder.transforms)
            .into_iter()
            .map(|(key, transform)| {
                let mut inputs = graph.inputs_for(&key);
                // Add a source as an input to every transform
                let source_key = OutputId::from(ComponentKey::from(format!(
                    "{}-{}",
                    key, "vector-unit-test-source"
                )));
                source_keys.push(source_key.clone());
                inputs.push(source_key);
                (
                    key,
                    transform.with_inputs(
                        inputs
                            .into_iter()
                            .map(|i| i.to_string())
                            .collect::<Vec<_>>(),
                    ),
                )
            })
            .collect::<IndexMap<_, _>>();

        // println!("refactor transforms {:?}\n", transforms);

        // mapping source key --> input events
        let mut source_to_events: IndexMap<ComponentKey, Vec<Event>> = IndexMap::new();
        for input in test.inputs {
            // todo: remove unwrap
            let event = build_input_event(&input).unwrap();
            // todo: add error if the insert_at doesn't exist
            let target_source_key = ComponentKey::from(format!(
                "{}-{}",
                input.insert_at.to_string(),
                "vector-unit-test-source"
            ));
            if let Some(events) = source_to_events.get_mut(&target_source_key) {
                events.push(event);
            } else {
                source_to_events.insert(target_source_key, vec![event]);
            }
        }

        // mapping source key --> transmitter
        let mut source_txs = IndexMap::new();
        // construct the sources
        let sources = source_keys
            .into_iter()
            .map(|id| {
                let (tx, rx) = tokio::sync::mpsc::channel(1);
                // todo: remove duplicate to_string
                source_txs.insert(ComponentKey::from(id.to_string()), tx);
                (
                    ComponentKey::from(id.to_string()),
                    SourceOuter::new(UnitTestSourceConfig {
                        ..Default::default()
                    }),
                )
            })
            .collect::<IndexMap<_, _>>();

        // mapping sink key --> receiver
        let mut sink_rxs = IndexMap::new();
        let sinks = transforms
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

                // For each possible output, create a sink
                keys.into_iter()
                    .map(|key| {
                        let inputs = vec![key.to_string()];
                        let sink_key = ComponentKey::from(format!(
                            "{}-{}",
                            key.replace(".", ""),
                            "vector-unit-test-sink"
                        ));
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        sink_rxs.insert(sink_key.clone(), rx);
                        (
                            sink_key,
                            SinkOuter::new(
                                inputs,
                                Box::new(UnitTestSinkConfig {
                                    result_tx: Arc::new(Mutex::new(Some(tx))),
                                    ..Default::default()
                                }),
                            ),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<IndexMap<_, _>>();

        // mapping sink key --> conditions for output events
        let mut sink_to_checks = IndexMap::new();
        for output in test.outputs {
            let sink_key = ComponentKey::from(format!(
                "{}-{}",
                output.extract_from.to_string().replace(".", ""),
                "vector-unit-test-sink"
            ));
            let mut conditions = Vec::new();
            // todo: use errors outside of this loop
            let mut errors = Vec::new();
            for (index, condition) in output.conditions.unwrap_or(Vec::new()).iter().enumerate() {
                match condition.build(&Default::default()) {
                    Ok(condition) => conditions.push(condition),
                    Err(error) => errors.push(format!(
                        "failed to create test condition '{}': {}",
                        index, error
                    )),
                }
            }
            // a check is a collection of conditions
            let mut checks = sink_to_checks.entry(sink_key).or_insert(vec![]);
            checks.push(conditions);
        }
        // println!("sinks to checks keys {:?}", sink_to_checks.keys());

        // println!("refactor -- made the following sinks: {:?}\n", sinks);

        builder.sources = sources;
        builder.transforms = transforms;
        builder.sinks = sinks;

        let config = builder.build().unwrap();
        let diff = config::ConfigDiff::initial(&config);
        let pieces = topology::build_or_log_errors(&config, &diff, HashMap::new())
            .await
            .unwrap();

        let (topology, _) = topology::start_validated(config, diff, pieces)
            .await
            .unwrap();
        // Send input events, drop any senders that will not be used to send events
        source_txs.retain(|key, _| source_to_events.get(key).is_some());
        for (key, events) in source_to_events {
            let tx = source_txs.get(&key).unwrap();
            for event in events {
                tx.send(event).await;
            }
        }
        drop(source_txs);
        let _ = topology.sources_finished().await;
        // let _ = tokio::spawn(async move { topology.stop().await });
        let _stop_complete = topology.stop();

        // Collect outputs
        // let mut in_flight = sink_rxs.into_iter().map(|(_, rx)| ReceiverStream::new(rx).collect::<Vec<_>>()).collect::<FuturesUnordered<_>>();
        let mut in_flight = sink_rxs
            .into_iter()
            .map(|(key, rx)| async move { (key, rx.await) })
            .collect::<FuturesUnordered<_>>();
        let mut test_result = UnitTestResult {
            name: test.name.clone(),
            test_errors: Vec::new(),
        };
        // while let Some((key, output_events)) = in_flight.next().await {
        //     let output_events = output_events.unwrap();
        //     // println!("received events from {:?} sink {:?}\n", key, output_events);
        //     // todo: move this checking logic out of this receiving loop to allow us to receive everything without delay first
        //     if let Some(checks) = sink_to_checks.get(&key) {
        //         // println!("checking...\n");
        //         // for each check, evaluate every event, breaking on the first event for which the check is entirely true
        //         for check in checks {
        //             let mut overall_check_errors = Vec::new();
        //             let mut result = false;
        //             for event in output_events.iter() {
        //                 // todo: add correct error message
        //                 let mut per_event_errors = Vec::new();
        //                 for condition in check {
        //                     match condition.check_with_context(event) {
        //                         Ok(_) => {}
        //                         Err(error) => {
        //                             per_event_errors.push(error);
        //                         }
        //                     }
        //                 }
        //                 if per_event_errors.is_empty() {
        //                     overall_check_errors.clear();
        //                     break;
        //                 } else {
        //                     overall_check_errors.extend(per_event_errors);
        //                 }
        //             }
        //             // either one or more events passed the check or the check failed for one or more events.
        //             // if failed, we need to update the test errors
        //             if !overall_check_errors.is_empty() {
        //                 test_result.test_errors.extend(overall_check_errors);
        //             }
        //         }
        //     }
        // }
        // test_results.push(test_result);
    }

    Ok(test_results)
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
                  type = "check_fields"
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
                  type = "check_fields"
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
                  type = "check_fields"
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
                  type = "check_fields"
                  "message.equals" = "not this"

              [[tests.outputs]]
                extract_from = "quz"
                [[tests.outputs.conditions]]
                  type = "check_fields"
                  "message.equals" = "not this"

            [[tests]]
              name = "broken test 2"

              [tests.input]
                insert_at = "nope"
                value = "nah this doesnt matter"

              [[tests.outputs]]
                extract_from = "quz"
                [[tests.outputs.conditions]]
                  type = "check_fields"
                  "message.equals" = "not this"

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
                  type = "check_fields"
                  "message.equals" = "not this"

              [[tests.outputs]]
                extract_from = "quz"
                [[tests.outputs.conditions]]
                  type = "check_fields"
                  "message.equals" = "not this"
        "#})
        .unwrap();

        let errs = build_unit_tests(config).await.err().unwrap();
        assert_eq!(
            errs,
            vec![
                r#"Failed to build test 'broken test':
  unable to complete topology between target transform 'foo' and output target 'baz'
  unable to complete topology between target transform 'foo' and output target 'quz'"#
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
                  type = "check_fields"
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
                  type = "check_fields"
                  "new_field.equals" = "string value"
                  "message.equals" = "nah this doesnt matter"

              [[tests.outputs]]
                extract_from = "foo_two"
                [[tests.outputs.conditions]]
                  type = "check_fields"
                  "new_field_two.equals" = "second string value"
                  "message.equals" = "nah this also doesnt matter"

              [[tests.outputs]]
                extract_from = "bar"
                [[tests.outputs.conditions]]
                  type = "check_fields"
                  "new_field.equals" = "string value"
                  "second_new_field.equals" = "also a string value"
                  "message.equals" = "nah this doesnt matter"
                [[tests.outputs.conditions]]
                  type = "check_fields"
                  "new_field_two.equals" = "second string value"
                  "second_new_field.equals" = "also a string value"
                  "message.equals" = "nah this also doesnt matter"

              [[tests.outputs]]
                extract_from = "baz"
                [[tests.outputs.conditions]]
                  type = "check_fields"
                  "new_field.equals" = "string value"
                  "second_new_field.equals" = "also a string value"
                  "third_new_field.equals" = "also also a string value"
                  "message.equals" = "nah this doesnt matter"
                [[tests.outputs.conditions]]
                  type = "check_fields"
                  "new_field_two.equals" = "second string value"
                  "second_new_field.equals" = "also a string value"
                  "third_new_field.equals" = "also also a string value"
                  "message.equals" = "nah this also doesnt matter"
        "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert_eq!(tests[0].run().1, Vec::<String>::new());
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
                  type = "check_fields"
                  "new_field.equals" = "string value"
                  "message.equals" = "nah this doesnt matter"

              [[tests.outputs]]
                extract_from = "bar"
                [[tests.outputs.conditions]]
                  type = "check_fields"
                  "new_field.equals" = "string value"
                  "second_new_field.equals" = "also a string value"
                  "message.equals" = "nah this doesnt matter"

              [[tests.outputs]]
                extract_from = "baz"
                [[tests.outputs.conditions]]
                  type = "check_fields"
                  "new_field.equals" = "string value"
                  "second_new_field.equals" = "also a string value"
                  "third_new_field.equals" = "also also a string value"
                  "message.equals" = "nah this doesnt matter"
        "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert_eq!(tests[0].run().1, Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_route() {
        let config: ConfigBuilder = toml::from_str(indoc! {r#"
            [transforms.foo]
              inputs = ["ignored"]
              type = "route"
              [transforms.foo.route.first]
                type = "check_fields"
                "message.eq" = "test swimlane 1"
              [transforms.foo.route.second]
                type = "check_fields"
                "message.eq" = "test swimlane 2"

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
                  type = "check_fields"
                  "message.equals" = "test swimlane 1"

              [[tests.outputs]]
                extract_from = "bar"
                [[tests.outputs.conditions]]
                  type = "check_fields"
                  "message.equals" = "test swimlane 1"
                  "new_field.equals" = "new field added"

            [[tests]]
              name = "successful route test 2"

              [tests.input]
                insert_at = "foo"
                value = "test swimlane 2"

              [[tests.outputs]]
                extract_from = "foo.second"
                [[tests.outputs.conditions]]
                  type = "check_fields"
                  "message.equals" = "test swimlane 2"
        "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert_eq!(tests[0].run().1, Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_fail_no_outputs() {
        let config: ConfigBuilder = toml::from_str(indoc! {r#"
            [transforms.foo]
              inputs = [ "TODO" ]
              type = "field_filter"
              field = "not_exist"
              value = "not_value"

              [[tests]]
                name = "check_no_outputs"
                [tests.input]
                  insert_at = "foo"
                  type = "raw"
                  value = "test value"

                [[tests.outputs]]
                  extract_from = "foo"
                  [[tests.outputs.conditions]]
                    type = "check_fields"
                    "message.equals" = "test value"
        "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert_ne!(tests[0].run().1, Vec::<String>::new());
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
                  type = "check_fields"
                  "baz.equals" = "new field 3"

                [[tests.outputs.conditions]]
                  type = "check_fields"
                  "bar.equals" = "new field 2"

            [[tests]]
              name = "check_multi_payloads_bad"

              [tests.input]
                insert_at = "foo"
                type = "raw"
                value = "first"

              [[tests.outputs]]
                extract_from = "boo"

                [[tests.outputs.conditions]]
                  type = "check_fields"
                  "baz.equals" = "new field 3"
                  "bar.equals" = "new field 2"
        "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert_eq!(tests[0].run().1, Vec::<String>::new());
        assert_ne!(tests[1].run().1, Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_no_outputs_from() {
        let config: ConfigBuilder = toml::from_str(indoc! {r#"
            [transforms.foo]
              inputs = [ "ignored" ]
              type = "field_filter"
              field = "message"
              value = "foo"

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
        assert_eq!(tests[0].run().1, Vec::<String>::new());
        assert_ne!(tests[1].run().1, Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_no_outputs_from_chained() {
        let config: ConfigBuilder = toml::from_str(indoc! { r#"
            [transforms.foo]
              inputs = [ "ignored" ]
              type = "field_filter"
              field = "message"
              value = "foo"

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
        assert_eq!(tests[0].run().1, Vec::<String>::new());
        assert_ne!(tests[1].run().1, Vec::<String>::new());
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
                  type = "check_fields"
                  "new_field.equals" = "string value"
                  "message.equals" = "this is the message"
                  "bool_val.eq" = true
                  "int_val.eq" = 5
        "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert_eq!(tests[0].run().1, Vec::<String>::new());
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
                  type = "check_fields"
                  "tagfoo.equals" = "valfoo"
                  "new_tag.eq" = "new value added"
        "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert_eq!(tests[0].run().1, Vec::<String>::new());
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
                  type = "check_fields"
                  "new_field.equals" = "string value"
                  "second_new_field.equals" = "also a string value"
                  "third_new_field.equals" = "also also a string value"
                  "message.equals" = "nah this doesnt matter"
        "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert_eq!(tests[0].run().1, Vec::<String>::new());
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
                  type = "check_fields"
                  "new_field.equals" = "string value"
                  "second_new_field.equals" = "also a string value"
                  "message.equals" = "nah this doesnt matter"

              [[tests.outputs]]
                extract_from = "baz"
                [[tests.outputs.conditions]]
                  type = "check_fields"
                  "new_field.equals" = "string value"
                  "second_new_field.equals" = "also also a string value"
                  "message.equals" = "nah this doesnt matter"
        "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert_eq!(tests[0].run().1, Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_fails() {
        let config: ConfigBuilder = toml::from_str(indoc! { r#"
            [transforms.foo]
              inputs = ["ignored"]
              type = "remove_fields"
              fields = ["timestamp"]

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
              name = "failing test"

              [tests.input]
                insert_at = "foo"
                value = "nah this doesnt matter"

              [[tests.outputs]]
                extract_from = "foo"
                [[tests.outputs.conditions]]
                  type = "check_fields"
                  "message.equals" = "nah this doesnt matter"

              [[tests.outputs]]
                extract_from = "bar"
                [[tests.outputs.conditions]]
                  type = "check_fields"
                  "message.equals" = "not this"
                [[tests.outputs.conditions]]
                  type = "check_fields"
                  "second_new_field.equals" = "and not this"

            [[tests]]
              name = "another failing test"

              [tests.input]
                insert_at = "foo"
                value = "also this doesnt matter"

              [[tests.outputs]]
                extract_from = "foo"
                [[tests.outputs.conditions]]
                  type = "check_fields"
                  "message.equals" = "also this doesnt matter"

              [[tests.outputs]]
                extract_from = "baz"
                [[tests.outputs.conditions]]
                  type = "check_fields"
                  "second_new_field.equals" = "nope not this"
                  "third_new_field.equals" = "and not this"
                  "message.equals" = "also this doesnt matter"
        "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert_ne!(tests[0].run().1, Vec::<String>::new());
        assert_ne!(tests[1].run().1, Vec::<String>::new());
        // TODO: The json representations are randomly ordered so these checks
        // don't always pass:
        /*
                assert_eq!(
                    tests[0].run().1,
                    vec![r#"check transform 'bar' failed conditions:
          condition[0]: predicates failed: [ message.equals: 'not this' ]
          condition[1]: predicates failed: [ second_new_field.equals: 'and not this' ]
        payloads (JSON encoded):
          input: {"message":"nah this doesnt matter"}
          output: {"message":"nah this doesnt matter","second_new_field":"also a string value"}"#.to_owned(),
                    ]);
                assert_eq!(
                    tests[1].run().1,
                    vec![r#"check transform 'baz' failed conditions:
          condition[0]: predicates failed: [ second_new_field.equals: 'nope not this', third_new_field.equals: 'and not this' ]
        payloads (JSON encoded):
          input: {"second_new_field":"also a string value","message":"also this doesnt matter"}
          output: {"third_new_field":"also also a string value","second_new_field":"also a string value","message":"also this doesnt matter"}"#.to_owned(),
                    ]);
                */
    }

    #[tokio::test]
    async fn type_inconsistency_while_expanding_transform() {
        let config: ConfigBuilder = toml::from_str(indoc! {r#"
            [sources.input]
              type = "demo_logs"
              format = "shuffle"
              lines = ["one", "two"]
              count = 5

            [transforms.foo]
              inputs = ["input"]
              type = "compound"
              [[transforms.foo.steps]]
                id = "step1"
                type = "log_to_metric"
                [[transforms.foo.steps.metrics]]
                  type = "counter"
                  field = "c"
                  name = "sum"
                  namespace = "ns"
              [[transforms.foo.steps]]
                id = "step2"
                type = "log_to_metric"
                [[transforms.foo.steps.metrics]]
                  type = "counter"
                  field = "c"
                  name = "sum"
                  namespace = "ns"

            [sinks.output]
              type = "console"
              inputs = [ "foo.step2" ]
              encoding = "json"
              target = "stdout"
        "#})
        .unwrap();

        let err = crate::config::compiler::compile(config).err().unwrap();
        assert_eq!(
            err,
            vec!["Data type mismatch between foo.step1 (Metric) and foo.step2 (Log)".to_owned()]
        );
    }

    #[tokio::test]
    async fn invalid_name_in_expanded_transform() {
        let config: ConfigBuilder = toml::from_str(indoc! {r#"
            [sources.input]
              type = "demo_logs"
              format = "shuffle"
              lines = ["one", "two"]
              count = 5

            [transforms.foo]
              inputs = ["input"]
              type = "compound"
              [[transforms.foo.steps]]
                type = "log_to_metric"
                [[transforms.foo.steps.metrics]]
                  type = "counter"
                  field = "c"
                  name = "sum"
                  namespace = "ns"
              [[transforms.foo.steps]]
                id = "0"
                type = "log_to_metric"
                [[transforms.foo.steps.metrics]]
                  type = "counter"
                  field = "c"
                  name = "sum"
                  namespace = "ns"

            [sinks.output]
              type = "console"
              inputs = [ "foo.0" ]
              encoding = "json"
              target = "stdout"
        "#})
        .unwrap();

        let err = crate::config::compiler::compile(config).err().unwrap();
        assert_eq!(
            err,
            vec![
                "failed to expand transform 'foo': conflicting id found while expanding transform"
                    .to_owned()
            ]
        );
    }
}
