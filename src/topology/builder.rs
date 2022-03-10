use std::{
    collections::HashMap,
    future::ready,
    sync::{Arc, Mutex},
    time::Instant,
};

use futures::{stream::FuturesOrdered, FutureExt, StreamExt};
use once_cell::sync::Lazy;
use stream_cancel::{StreamExt as StreamCancelExt, Trigger, Tripwire};
use tokio::{
    select,
    time::{timeout, Duration},
};
use tracing_futures::Instrument;
use vector_core::{
    buffers::{
        topology::{
            builder::TopologyBuilder,
            channel::{BufferReceiver, BufferSender},
        },
        BufferType, WhenFull,
    },
    internal_event::EventsSent,
    ByteSizeOf,
};

use super::{
    fanout::{self, Fanout},
    schema,
    task::{Task, TaskOutput},
    BuiltBuffer, ConfigDiff,
};
use crate::{
    config::{
        ComponentKey, DataType, Input, Output, OutputId, ProxyConfig, SinkContext, SourceContext,
        TransformContext,
    },
    event::{EventArray, EventContainer},
    internal_events::EventsReceived,
    shutdown::SourceShutdownCoordinator,
    transforms::{SyncTransform, TaskTransform, Transform, TransformOutputs, TransformOutputsBuf},
    SourceSender,
};

static ENRICHMENT_TABLES: Lazy<enrichment::TableRegistry> =
    Lazy::new(enrichment::TableRegistry::default);

pub(crate) const SOURCE_SENDER_BUFFER_SIZE: usize = 1000;

static TRANSFORM_CONCURRENCY_LIMIT: Lazy<usize> = Lazy::new(|| {
    crate::app::WORKER_THREADS
        .get()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or_else(num_cpus::get)
});

pub(self) async fn load_enrichment_tables<'a>(
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
    pub(super) inputs: HashMap<ComponentKey, (BufferSender<EventArray>, Vec<OutputId>)>,
    pub(crate) outputs: HashMap<ComponentKey, HashMap<Option<String>, fanout::ControlChannel>>,
    pub(super) tasks: HashMap<ComponentKey, Task>,
    pub(crate) source_tasks: HashMap<ComponentKey, Task>,
    pub(super) healthchecks: HashMap<ComponentKey, Task>,
    pub(crate) shutdown_coordinator: SourceShutdownCoordinator,
    pub(crate) detach_triggers: HashMap<ComponentKey, Trigger>,
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
        let typetag = source.inner.source_type();
        let source_outputs = source.inner.outputs();

        let mut builder = SourceSender::builder().with_buffer(SOURCE_SENDER_BUFFER_SIZE);
        let mut pumps = Vec::new();
        let mut controls = HashMap::new();
        let mut schema_definitions = HashMap::with_capacity(source_outputs.len());

        for output in source_outputs {
            let mut rx = builder.add_output(output.clone());

            let (mut fanout, control) = Fanout::new();
            let pump = async move {
                while let Some(array) = rx.next().await {
                    fanout.send(array).await;
                }
                Ok(TaskOutput::Source)
            };

            pumps.push(pump);
            controls.insert(
                OutputId {
                    component: key.clone(),
                    port: output.port.clone(),
                },
                control,
            );

            let schema_definition = output
                .log_schema_definition
                .unwrap_or_else(schema::Definition::empty);

            schema_definitions.insert(output.port, schema_definition);
        }

        let pump = async move {
            let mut handles = Vec::new();
            for pump in pumps {
                handles.push(tokio::spawn(pump));
            }
            for handle in handles {
                handle.await.expect("join error")?;
            }
            Ok(TaskOutput::Source)
        };
        let pump = Task::new(key.clone(), typetag, pump);

        let pipeline = builder.build();

        let (shutdown_signal, force_shutdown_tripwire) = shutdown_coordinator.register_source(key);

        let context = SourceContext {
            key: key.clone(),
            globals: config.global.clone(),
            shutdown: shutdown_signal,
            out: pipeline,
            proxy: ProxyConfig::merge_with_env(&config.global.proxy, &source.proxy),
            acknowledgements: source.sink_acknowledgements,
            schema_definitions,
        };
        let server = match source.inner.build(context).await {
            Err(error) => {
                errors.push(format!("Source \"{}\": {}", key, error));
                continue;
            }
            Ok(server) => server,
        };

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

        outputs.extend(controls);
        tasks.insert(key.clone(), pump);
        source_tasks.insert(key.clone(), server);
    }

    let mut definition_cache = HashMap::default();

    // Build transforms
    for (key, transform) in config
        .transforms
        .iter()
        .filter(|(key, _)| diff.transforms.contains_new(key))
    {
        let mut schema_definitions = HashMap::new();
        let merged_definition = if config.schema.enabled {
            schema::merged_definition(&transform.inputs, config, &mut definition_cache)
        } else {
            schema::Definition::empty()
        };

        for output in transform.inner.outputs(&merged_definition) {
            let definition = match output.log_schema_definition {
                Some(definition) => definition,
                None => merged_definition.clone(),
            };

            schema_definitions.insert(output.port, definition);
        }

        let context = TransformContext {
            key: Some(key.clone()),
            globals: config.global.clone(),
            enrichment_tables: enrichment_tables.clone(),
            schema_definitions,
            merged_schema_definition: merged_definition.clone(),
        };

        let node = TransformNode {
            key: key.clone(),
            typetag: transform.inner.transform_type(),
            inputs: transform.inputs.clone(),
            input_details: transform.inner.input(),
            outputs: transform.inner.outputs(&merged_definition),
            enable_concurrency: transform.inner.enable_concurrency(),
        };

        let transform = match transform.inner.build(&context).await {
            Err(error) => {
                errors.push(format!("Transform \"{}\": {}", key, error));
                continue;
            }
            Ok(transform) => transform,
        };

        let (input_tx, input_rx) = TopologyBuilder::standalone_memory(100, WhenFull::Block).await;

        inputs.insert(key.clone(), (input_tx, node.inputs.clone()));

        let (transform_task, transform_outputs) = build_transform(transform, node, input_rx);

        outputs.extend(transform_outputs);
        tasks.insert(key.clone(), transform_task);
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
        let input_type = sink.inner.input().data_type();

        let (tx, rx, acker) = if let Some(buffer) = buffers.remove(key) {
            buffer
        } else {
            let buffer_type = match sink.buffer.stages().first().expect("cant ever be empty") {
                BufferType::Memory { .. } => "memory",
                BufferType::DiskV1 { .. } | BufferType::DiskV2 { .. } => "disk",
            };
            let buffer_span = error_span!(
                "sink",
                component_kind = "sink",
                component_id = %key.id(),
                component_type = typetag,
                component_name = %key.id(),
                buffer_type = buffer_type,
            );
            let buffer = sink
                .buffer
                .build(config.global.data_dir.clone(), key.to_string(), buffer_span)
                .await;
            match buffer {
                Err(error) => {
                    errors.push(format!("Sink \"{}\": {}", key, error));
                    continue;
                }
                Ok((tx, rx, acker)) => (tx, Arc::new(Mutex::new(Some(rx))), acker),
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
                    .filter(|events: &EventArray| ready(filter_events_type(events, input_type)))
                    .inspect(|events| {
                        emit!(&EventsReceived {
                            count: events.len(),
                            byte_size: events.size_of(),
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
    enrichment_tables.finish_load();

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
        };

        Ok(pieces)
    } else {
        Err(errors)
    }
}

const fn filter_events_type(events: &EventArray, data_type: DataType) -> bool {
    match events {
        EventArray::Logs(_) => data_type.contains(DataType::Log),
        EventArray::Metrics(_) => data_type.contains(DataType::Metric),
        EventArray::Traces(_) => data_type.contains(DataType::Trace),
    }
}

#[derive(Debug, Clone)]
struct TransformNode {
    key: ComponentKey,
    typetag: &'static str,
    inputs: Vec<OutputId>,
    input_details: Input,
    outputs: Vec<Output>,
    enable_concurrency: bool,
}

fn build_transform(
    transform: Transform,
    node: TransformNode,
    input_rx: BufferReceiver<EventArray>,
) -> (Task, HashMap<OutputId, fanout::ControlChannel>) {
    match transform {
        // TODO: avoid the double boxing for function transforms here
        Transform::Function(t) => build_sync_transform(Box::new(t), node, input_rx),
        Transform::Synchronous(t) => build_sync_transform(t, node, input_rx),
        Transform::Task(t) => build_task_transform(
            t,
            input_rx,
            node.input_details.data_type(),
            node.typetag,
            &node.key,
        ),
    }
}

fn build_sync_transform(
    t: Box<dyn SyncTransform>,
    node: TransformNode,
    input_rx: BufferReceiver<EventArray>,
) -> (Task, HashMap<OutputId, fanout::ControlChannel>) {
    let (outputs, controls) = TransformOutputs::new(node.outputs);

    let runner = Runner::new(t, input_rx, node.input_details.data_type(), outputs);
    let transform = if node.enable_concurrency {
        runner.run_concurrently().boxed()
    } else {
        runner.run_inline().boxed()
    };

    let mut output_controls = HashMap::new();
    for (name, control) in controls {
        let id = name
            .map(|name| OutputId::from((&node.key, name)))
            .unwrap_or_else(|| OutputId::from(&node.key));
        output_controls.insert(id, control);
    }

    let task = Task::new(node.key.clone(), node.typetag, transform);

    (task, output_controls)
}

struct Runner {
    transform: Box<dyn SyncTransform>,
    input_rx: Option<BufferReceiver<EventArray>>,
    input_type: DataType,
    outputs: TransformOutputs,
    timer: crate::utilization::Timer,
    last_report: Instant,
}

impl Runner {
    fn new(
        transform: Box<dyn SyncTransform>,
        input_rx: BufferReceiver<EventArray>,
        input_type: DataType,
        outputs: TransformOutputs,
    ) -> Self {
        Self {
            transform,
            input_rx: Some(input_rx),
            input_type,
            outputs,
            timer: crate::utilization::Timer::new(),
            last_report: Instant::now(),
        }
    }

    fn on_events_received(&mut self, events: &EventArray) {
        let stopped = self.timer.stop_wait();
        if stopped.duration_since(self.last_report).as_secs() >= 5 {
            self.timer.report();
            self.last_report = stopped;
        }

        emit!(&EventsReceived {
            count: events.len(),
            byte_size: events.size_of(),
        });
    }

    async fn send_outputs(&mut self, outputs_buf: &mut TransformOutputsBuf) {
        self.timer.start_wait();
        self.outputs.send(outputs_buf).await;
    }

    async fn run_inline(mut self) -> Result<TaskOutput, ()> {
        // 128 is an arbitrary, smallish constant
        const INLINE_BATCH_SIZE: usize = 128;

        let mut outputs_buf = self.outputs.new_buf_with_capacity(INLINE_BATCH_SIZE);

        let mut input_rx = self
            .input_rx
            .take()
            .expect("can't run runner twice")
            .filter(move |events| ready(filter_events_type(events, self.input_type)));

        self.timer.start_wait();
        while let Some(events) = input_rx.next().await {
            self.on_events_received(&events);
            self.transform.transform_all(events, &mut outputs_buf);
            self.send_outputs(&mut outputs_buf).await;
        }

        debug!("Finished.");
        Ok(TaskOutput::Transform)
    }

    async fn run_concurrently(mut self) -> Result<TaskOutput, ()> {
        let mut input_rx = self
            .input_rx
            .take()
            .expect("can't run runner twice")
            .filter(move |events| ready(filter_events_type(events, self.input_type)));

        let mut in_flight = FuturesOrdered::new();
        let mut shutting_down = false;

        self.timer.start_wait();
        loop {
            tokio::select! {
                biased;

                result = in_flight.next(), if !in_flight.is_empty() => {
                    match result {
                        Some(Ok(outputs_buf)) => {
                            let mut outputs_buf: TransformOutputsBuf = outputs_buf;
                            self.send_outputs(&mut outputs_buf).await;
                        }
                        _ => unreachable!("join error or bad poll"),
                    }
                }

                input_events = input_rx.next(), if in_flight.len() < *TRANSFORM_CONCURRENCY_LIMIT && !shutting_down => {
                    match input_events {
                        Some(events) => {
                            self.on_events_received(&events);

                            let mut t = self.transform.clone();
                            let mut outputs_buf = self.outputs.new_buf_with_capacity(events.len());
                            let task = tokio::spawn(async move {
                                t.transform_all(events, &mut outputs_buf);
                                outputs_buf
                            }.in_current_span());
                            in_flight.push(task);
                        }
                        None => {
                            shutting_down = true;
                            continue
                        }
                    }
                }

                else => {
                    if shutting_down {
                        break
                    }
                }
            }
        }

        debug!("Finished.");
        Ok(TaskOutput::Transform)
    }
}

fn build_task_transform(
    t: Box<dyn TaskTransform<EventArray>>,
    input_rx: BufferReceiver<EventArray>,
    input_type: DataType,
    typetag: &str,
    key: &ComponentKey,
) -> (Task, HashMap<OutputId, fanout::ControlChannel>) {
    let (mut fanout, control) = Fanout::new();

    let input_rx = crate::utilization::wrap(input_rx);

    let filtered = input_rx
        .filter(move |events| ready(filter_events_type(events, input_type)))
        .inspect(|events| {
            emit!(&EventsReceived {
                count: events.len(),
                byte_size: events.size_of(),
            })
        });
    let stream = t
        .transform(Box::pin(filtered))
        .inspect(|events: &EventArray| {
            emit!(&EventsSent {
                count: events.len(),
                byte_size: events.size_of(),
                output: None,
            });
        });
    let transform = async move {
        fanout.send_stream(stream).await;
        debug!("Finished.");
        Ok(TaskOutput::Transform)
    }
    .boxed();

    let mut outputs = HashMap::new();
    outputs.insert(OutputId::from(key), control);

    let task = Task::new(key.clone(), typetag, transform);

    (task, outputs)
}
