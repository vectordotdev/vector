use super::{
    fanout::{self, Fanout},
    task::{Task, TaskOutput},
    BuiltBuffer, ConfigDiff,
};
use crate::{
    buffers,
    config::{
        ComponentKey, DataType, OutputId, ProxyConfig, SinkContext, SourceContext, TransformContext,
    },
    event::Event,
    internal_events::{EventsReceived, EventsSent},
    shutdown::SourceShutdownCoordinator,
    transforms::Transform,
    Pipeline,
};
use futures::{stream, FutureExt, SinkExt, StreamExt, TryFutureExt};
use lazy_static::lazy_static;
use std::pin::Pin;
use std::{
    collections::HashMap,
    future::ready,
    sync::{Arc, Mutex},
};
use stream_cancel::{StreamExt as StreamCancelExt, Trigger, Tripwire};
use tokio::{
    select,
    time::{timeout, Duration},
};
use vector_core::ByteSizeOf;

lazy_static! {
    static ref ENRICHMENT_TABLES: enrichment::TableRegistry = enrichment::TableRegistry::default();
}

pub async fn load_enrichment_tables<'a>(
    config: &'a super::Config,
    diff: &'a ConfigDiff,
) -> (&'static enrichment::TableRegistry, Vec<String>) {
    let mut enrichment_tables = HashMap::new();

    let mut errors = vec![];

    // Build enrichment tables
    'tables: for (name, table) in config.enrichment_tables.iter() {
        let table_name = name.to_string();
        if ENRICHMENT_TABLES.needs_reload(&table_name) {
            let indexes = if !diff.enrichment_tables.contains_new(name) {
                // If this is an existing enrichment table, we need to store the indexes to reapply
                // them again post load.
                Some(ENRICHMENT_TABLES.index_fields(&table_name))
            } else {
                None
            };

            let mut table = match table.inner.build(&config.global).await {
                Ok(table) => table,
                Err(error) => {
                    errors.push(format!("Enrichment Table \"{}\": {}", name, error));
                    continue;
                }
            };

            if let Some(indexes) = indexes {
                for (case, index) in indexes {
                    match table
                        .add_index(case, &index.iter().map(|s| s.as_ref()).collect::<Vec<_>>())
                    {
                        Ok(_) => (),
                        Err(error) => {
                            // If there is an error adding an index we do not want to use the reloaded
                            // data, the previously loaded data will still need to be used.
                            // Just report the error and continue.
                            error!(message = "Unable to add index to reloaded enrichment table.",
                                    table = ?name.to_string(),
                                    %error);
                            continue 'tables;
                        }
                    }
                }
            }

            enrichment_tables.insert(table_name, table);
        }
    }

    ENRICHMENT_TABLES.load(enrichment_tables);

    (&ENRICHMENT_TABLES, errors)
}

pub struct Pieces {
    pub inputs: HashMap<ComponentKey, (buffers::BufferInputCloner<Event>, Vec<OutputId>)>,
    pub outputs: HashMap<ComponentKey, HashMap<Option<String>, fanout::ControlChannel>>,
    pub tasks: HashMap<ComponentKey, Task>,
    pub source_tasks: HashMap<ComponentKey, Task>,
    pub healthchecks: HashMap<ComponentKey, Task>,
    pub shutdown_coordinator: SourceShutdownCoordinator,
    pub detach_triggers: HashMap<ComponentKey, Trigger>,
    pub enrichment_tables: enrichment::TableRegistry,
}

/// Builds only the new pieces, and doesn't check their topology.
pub async fn build_pieces(
    config: &super::Config,
    diff: &ConfigDiff,
    mut buffers: HashMap<ComponentKey, BuiltBuffer>,
) -> Result<Pieces, Vec<String>> {
    let mut inputs = HashMap::new();
    let mut outputs = HashMap::new();
    let mut tasks = HashMap::new();
    let mut source_tasks = HashMap::new();
    let mut healthchecks = HashMap::new();
    let mut shutdown_coordinator = SourceShutdownCoordinator::default();
    let mut detach_triggers = HashMap::new();

    let mut errors = vec![];

    let (enrichment_tables, enrichment_errors) = load_enrichment_tables(config, diff).await;
    errors.extend(enrichment_errors);

    // Build sources
    for (key, source) in config
        .sources
        .iter()
        .filter(|(key, _)| diff.sources.contains_new(key))
    {
        let (tx, rx) = futures::channel::mpsc::channel(1000);
        let pipeline = Pipeline::from_sender(tx, vec![]);

        let typetag = source.inner.source_type();

        let (shutdown_signal, force_shutdown_tripwire) = shutdown_coordinator.register_source(key);

        let context = SourceContext {
            key: key.clone(),
            globals: config.global.clone(),
            shutdown: shutdown_signal,
            out: pipeline,
            acknowledgements: source.acknowledgements,
            proxy: ProxyConfig::merge_with_env(&config.global.proxy, &source.proxy),
        };
        let server = match source.inner.build(context).await {
            Err(error) => {
                errors.push(format!("Source \"{}\": {}", key, error));
                continue;
            }
            Ok(server) => server,
        };

        let (output, control) = Fanout::new();
        let pump = rx.map(Ok).forward(output).map_ok(|_| TaskOutput::Source);
        let pump = Task::new(key.clone(), typetag, pump);

        // The force_shutdown_tripwire is a Future that when it resolves means that this source
        // has failed to shut down gracefully within its allotted time window and instead should be
        // forcibly shut down. We accomplish this by select()-ing on the server Task with the
        // force_shutdown_tripwire. That means that if the force_shutdown_tripwire resolves while
        // the server Task is still running the Task will simply be dropped on the floor.
        let server = async {
            let result = select! {
                biased;

                _ = force_shutdown_tripwire => {
                    Ok(())
                },
                result = server => result,
            };

            match result {
                Ok(()) => {
                    debug!("Finished.");
                    Ok(TaskOutput::Source)
                }
                Err(()) => Err(()),
            }
        };
        let server = Task::new(key.clone(), typetag, server);

        outputs.insert(OutputId::from(key), control);
        tasks.insert(key.clone(), pump);
        source_tasks.insert(key.clone(), server);
    }

    let context = TransformContext {
        globals: config.global.clone(),
        enrichment_tables: enrichment_tables.clone(),
    };

    // Build transforms
    for (key, transform) in config
        .transforms
        .iter()
        .filter(|(key, _)| diff.transforms.contains_new(key))
    {
        let trans_inputs = &transform.inputs;

        let typetag = transform.inner.transform_type();

        let mut named_outputs = transform.inner.named_outputs();

        let input_type = transform.inner.input_type();
        let transform = match transform.inner.build(&context).await {
            Err(error) => {
                errors.push(format!("Transform \"{}\": {}", key, error));
                continue;
            }
            Ok(transform) => transform,
        };

        let (input_tx, input_rx, _) = vector_core::buffers::build(
            vector_core::buffers::Variant::Memory {
                max_events: 100,
                when_full: vector_core::buffers::WhenFull::Block,
                instrument: false,
            },
            tracing::Span::none(),
        )
        .unwrap();
        let mut input_rx = crate::utilization::wrap(Pin::new(input_rx));

        let task = match transform {
            Transform::Function(mut t) => {
                let (output, control) = Fanout::new();

                let transform = input_rx
                    .filter(move |event| ready(filter_event_type(event, input_type)))
                    .ready_chunks(128) // 128 is an arbitrary, smallish constant
                    .inspect(|events| {
                        emit!(&EventsReceived {
                            count: events.len(),
                            byte_size: events.iter().map(|e| e.size_of()).sum(),
                        });
                    })
                    .flat_map(move |events| {
                        let mut output = Vec::with_capacity(events.len());
                        let mut buf = Vec::with_capacity(4); // also an arbitrary,
                                                             // smallish constant
                        for v in events {
                            t.transform(&mut buf, v);
                            output.append(&mut buf);
                        }
                        emit!(&EventsSent {
                            count: output.len(),
                            byte_size: output.iter().map(|event| event.size_of()).sum(),
                        });
                        stream::iter(output.into_iter()).map(Ok)
                    })
                    .forward(output)
                    .boxed()
                    .map_ok(|_| {
                        debug!("Finished.");
                        TaskOutput::Transform
                    });

                outputs.insert(OutputId::from(key), control);

                Task::new(key.clone(), typetag, transform)
            }
            Transform::FallibleFunction(mut t) => {
                let (mut output, control) = Fanout::new();
                let (mut errors_output, errors_control) = Fanout::new();

                let transform = async move {
                    while let Some(event) = input_rx.next().await {
                        if !filter_event_type(&event, input_type) {
                            continue;
                        }
                        emit!(&EventsReceived {
                            count: 1,
                            byte_size: event.size_of(),
                        });

                        let mut buf = Vec::with_capacity(1);
                        let mut err_buf = Vec::with_capacity(1);

                        t.transform(&mut buf, &mut err_buf, event);
                        // TODO: account for error outputs separately?
                        emit!(&EventsSent {
                            count: buf.len() + err_buf.len(),
                            byte_size: buf.iter().map(|event| event.size_of()).sum::<usize>()
                                + err_buf.iter().map(|event| event.size_of()).sum::<usize>(),
                        });

                        for event in buf {
                            output.feed(event).await.expect("unit error");
                        }
                        output.flush().await.expect("unit error");
                        for event in err_buf {
                            errors_output.feed(event).await.expect("unit error");
                        }
                        errors_output.flush().await.expect("unit error");
                    }

                    debug!("Finished.");
                    Ok(TaskOutput::Transform)
                }
                .boxed();

                outputs.insert(OutputId::from(key), control);
                // TODO: actually drive fanout creation from transform output declaration instead
                // of relying on the one fallible function pattern we currently have
                assert_eq!(1, named_outputs.len());
                outputs.insert(
                    OutputId::from((key, named_outputs.remove(0))),
                    errors_control,
                );

                Task::new(key.clone(), typetag, transform)
            }
            Transform::Task(t) => {
                let (output, control) = Fanout::new();

                let filtered = input_rx
                    .filter(move |event| ready(filter_event_type(event, input_type)))
                    .inspect(|event| {
                        emit!(&EventsReceived {
                            count: 1,
                            byte_size: event.size_of(),
                        })
                    });
                let transform = t
                    .transform(Box::pin(filtered))
                    .map(Ok)
                    .forward(output.with(|event: Event| async {
                        emit!(&EventsSent {
                            count: 1,
                            byte_size: event.size_of(),
                        });
                        Ok(event)
                    }))
                    .boxed()
                    .map_ok(|_| {
                        debug!("Finished.");
                        TaskOutput::Transform
                    });

                outputs.insert(OutputId::from(key), control);

                Task::new(key.clone(), typetag, transform)
            }
        };

        inputs.insert(key.clone(), (input_tx, trans_inputs.clone()));
        tasks.insert(key.clone(), task);
    }

    // Build sinks
    for (key, sink) in config
        .sinks
        .iter()
        .filter(|(key, _)| diff.sinks.contains_new(key))
    {
        let sink_inputs = &sink.inputs;
        let healthcheck = sink.healthcheck();
        let enable_healthcheck = healthcheck.enabled && config.healthchecks.enabled;

        let typetag = sink.inner.sink_type();
        let input_type = sink.inner.input_type();

        let (tx, rx, acker) = if let Some(buffer) = buffers.remove(key) {
            buffer
        } else {
            let buffer_type = match sink.buffer {
                buffers::BufferConfig::Memory { .. } => "memory",
                buffers::BufferConfig::Disk { .. } => "disk",
            };
            let buffer_span = error_span!(
                "sink",
                component_kind = "sink",
                component_id = %key.id(),
                component_scope = %key.scope(),
                component_type = typetag,
                component_name = %key.id(),
                buffer_type = buffer_type,
            );
            let buffer = sink.buffer.build(&config.global.data_dir, key, buffer_span);
            match buffer {
                Err(error) => {
                    errors.push(format!("Sink \"{}\": {}", key, error));
                    continue;
                }
                Ok((tx, rx, acker)) => (tx, Arc::new(Mutex::new(Some(rx.into()))), acker),
            }
        };

        let cx = SinkContext {
            acker: acker.clone(),
            healthcheck,
            globals: config.global.clone(),
            proxy: ProxyConfig::merge_with_env(&config.global.proxy, sink.proxy()),
        };

        let (sink, healthcheck) = match sink.inner.build(cx).await {
            Err(error) => {
                errors.push(format!("Sink \"{}\": {}", key, error));
                continue;
            }
            Ok(built) => built,
        };

        let (trigger, tripwire) = Tripwire::new();

        let sink = async move {
            // Why is this Arc<Mutex<Option<_>>> needed you ask.
            // In case when this function build_pieces errors
            // this future won't be run so this rx won't be taken
            // which will enable us to reuse rx to rebuild
            // old configuration by passing this Arc<Mutex<Option<_>>>
            // yet again.
            let rx = rx
                .lock()
                .unwrap()
                .take()
                .expect("Task started but input has been taken.");

            let mut rx = crate::utilization::wrap(rx);

            sink.run(
                rx.by_ref()
                    .filter(|event| ready(filter_event_type(event, input_type)))
                    .inspect(|event| {
                        emit!(&EventsReceived {
                            count: 1,
                            byte_size: event.size_of(),
                        })
                    })
                    .take_until_if(tripwire),
            )
            .await
            .map(|_| {
                debug!("Finished.");
                TaskOutput::Sink(rx, acker)
            })
        };

        let task = Task::new(key.clone(), typetag, sink);

        let component_key = key.clone();
        let healthcheck_task = async move {
            if enable_healthcheck {
                let duration = Duration::from_secs(10);
                timeout(duration, healthcheck)
                    .map(|result| match result {
                        Ok(Ok(_)) => {
                            info!("Healthcheck: Passed.");
                            Ok(TaskOutput::Healthcheck)
                        }
                        Ok(Err(error)) => {
                            error!(
                                msg = "Healthcheck: Failed Reason.",
                                %error,
                                component_kind = "sink",
                                component_type = typetag,
                                component_id = %component_key.id(),
                                // maintained for compatibility
                                component_name = %component_key.id(),
                            );
                            Err(())
                        }
                        Err(_) => {
                            error!(
                                msg = "Healthcheck: timeout.",
                                component_kind = "sink",
                                component_type = typetag,
                                component_id = %component_key.id(),
                                // maintained for compatibility
                                component_name = %component_key.id(),
                            );
                            Err(())
                        }
                    })
                    .await
            } else {
                info!("Healthcheck: Disabled.");
                Ok(TaskOutput::Healthcheck)
            }
        };

        let healthcheck_task = Task::new(key.clone(), typetag, healthcheck_task);

        inputs.insert(key.clone(), (tx, sink_inputs.clone()));
        healthchecks.insert(key.clone(), healthcheck_task);
        tasks.insert(key.clone(), task);
        detach_triggers.insert(key.clone(), trigger);
    }

    // We should have all the data for the enrichment tables loaded now, so switch them over to
    // readonly.
    ENRICHMENT_TABLES.finish_load();

    let mut finalized_outputs = HashMap::new();
    for (id, output) in outputs {
        let entry = finalized_outputs
            .entry(id.component)
            .or_insert_with(HashMap::new);
        entry.insert(id.port, output);
    }

    if errors.is_empty() {
        let pieces = Pieces {
            inputs,
            outputs: finalized_outputs,
            tasks,
            source_tasks,
            healthchecks,
            shutdown_coordinator,
            detach_triggers,
            enrichment_tables: ENRICHMENT_TABLES.clone(),
        };

        Ok(pieces)
    } else {
        Err(errors)
    }
}

const fn filter_event_type(event: &Event, data_type: DataType) -> bool {
    match data_type {
        DataType::Any => true,
        DataType::Log => matches!(event, Event::Log(_)),
        DataType::Metric => matches!(event, Event::Metric(_)),
    }
}
