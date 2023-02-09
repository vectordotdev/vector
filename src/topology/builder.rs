use std::{
    collections::HashMap,
    future::ready,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
    time::Instant,
};

use futures::{stream::FuturesOrdered, FutureExt, StreamExt, TryStreamExt};
use futures_util::stream::FuturesUnordered;
use once_cell::sync::Lazy;
use stream_cancel::{StreamExt as StreamCancelExt, Trigger, Tripwire};
use tokio::{
    select,
    sync::oneshot,
    time::{timeout, Duration},
};
use tracing::Instrument;
use vector_common::internal_event::{
    self, CountByteSize, EventsSent, InternalEventHandle as _, Registered,
};
use vector_config::NamedComponent;
use vector_core::config::LogNamespace;
use vector_core::{
    buffers::{
        topology::{
            builder::TopologyBuilder,
            channel::{BufferReceiver, BufferSender},
        },
        BufferType, WhenFull,
    },
    schema::Definition,
    EstimatedJsonEncodedSizeOf,
};

use super::{
    fanout::{self, Fanout},
    schema,
    task::{Task, TaskOutput, TaskResult},
    BuiltBuffer, ConfigDiff,
};
use crate::{
    config::{
        ComponentKey, DataType, EnrichmentTableConfig, Input, Inputs, Output, OutputId,
        ProxyConfig, SinkConfig, SinkContext, SourceConfig, SourceContext, TransformConfig,
        TransformContext, TransformOuter,
    },
    event::{EventArray, EventContainer},
    internal_events::EventsReceived,
    shutdown::SourceShutdownCoordinator,
    source_sender::CHUNK_SIZE,
    spawn_named,
    topology::task::TaskError,
    transforms::{SyncTransform, TaskTransform, Transform, TransformOutputs, TransformOutputsBuf},
    utilization::wrap,
    SourceSender,
};

static ENRICHMENT_TABLES: Lazy<enrichment::TableRegistry> =
    Lazy::new(enrichment::TableRegistry::default);

pub(crate) static SOURCE_SENDER_BUFFER_SIZE: Lazy<usize> =
    Lazy::new(|| *TRANSFORM_CONCURRENCY_LIMIT * CHUNK_SIZE);

const READY_ARRAY_CAPACITY: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(CHUNK_SIZE * 4) };
pub(crate) const TOPOLOGY_BUFFER_SIZE: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(100) };

static TRANSFORM_CONCURRENCY_LIMIT: Lazy<usize> = Lazy::new(|| {
    crate::app::WORKER_THREADS
        .get()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or_else(crate::num_threads)
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
            let indexes = if !diff.enrichment_tables.is_added(name) {
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
    pub(super) inputs: HashMap<ComponentKey, (BufferSender<EventArray>, Inputs<OutputId>)>,
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
        .sources()
        .filter(|(key, _)| diff.sources.contains_new(key))
    {
        debug!(component = %key, "Building new source.");

        let typetag = source.inner.get_component_name();
        let source_outputs = source.inner.outputs(config.schema.log_namespace());

        let span = error_span!(
            "source",
            component_kind = "source",
            component_id = %key.id(),
            component_type = %source.inner.get_component_name(),
            // maintained for compatibility
            component_name = %key.id(),
        );
        let task_name = format!(
            ">> {} ({}, pump) >>",
            source.inner.get_component_name(),
            key.id()
        );

        let mut builder = {
            let _span = span.enter();
            SourceSender::builder().with_buffer(*SOURCE_SENDER_BUFFER_SIZE)
        };
        let mut pumps = Vec::new();
        let mut controls = HashMap::new();
        let mut schema_definitions = HashMap::with_capacity(source_outputs.len());

        for output in source_outputs {
            let mut rx = builder.add_output(output.clone());

            let (mut fanout, control) = Fanout::new();
            let pump = async move {
                debug!("Source pump starting.");

                while let Some(array) = rx.next().await {
                    fanout.send(array).await.map_err(|e| {
                        debug!("Source pump finished with an error.");
                        TaskError::wrapped(e)
                    })?;
                }

                debug!("Source pump finished normally.");
                Ok(TaskOutput::Source)
            };

            pumps.push(pump.instrument(span.clone()));
            controls.insert(
                OutputId {
                    component: key.clone(),
                    port: output.port.clone(),
                },
                control,
            );

            let schema_definition = output
                .log_schema_definition
                .unwrap_or_else(schema::Definition::default_legacy_namespace);

            schema_definitions.insert(output.port, schema_definition);
        }

        let (pump_error_tx, mut pump_error_rx) = oneshot::channel();
        let pump = async move {
            debug!("Source pump supervisor starting.");

            // Spawn all of the per-output pumps and then await their completion.
            //
            // If any of the pumps complete with an error, or panic/are cancelled, we return
            // immediately.
            let mut handles = FuturesUnordered::new();
            for pump in pumps {
                handles.push(spawn_named(pump, task_name.as_ref()));
            }

            let mut had_pump_error = false;
            while let Some(output) = handles.try_next().await? {
                if let Err(e) = output {
                    // Immediately send the error to the source's wrapper future, but ignore any
                    // errors during the send, since nested errors wouldn't make any sense here.
                    let _ = pump_error_tx.send(e);
                    had_pump_error = true;
                    break;
                }
            }

            if had_pump_error {
                debug!("Source pump supervisor task finished with an error.");
            } else {
                debug!("Source pump supervisor task finished normally.");
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
            schema: config.schema,
        };
        let server = match source.inner.build(context).await {
            Err(error) => {
                errors.push(format!("Source \"{}\": {}", key, error));
                continue;
            }
            Ok(server) => server,
        };

        // Build a wrapper future that drives the actual source future, but returns early if we've
        // been signalled to forcefully shutdown, or if the source pump encounters an error.
        //
        // The forceful shutdown will only resolve if the source itself doesn't shutdown gracefully
        // within the alloted time window. This can occur normally for certain sources, like stdin,
        // where the I/O is blocking (in a separate thread) and won't wake up to check if it's time
        // to shutdown unless some input is given.
        let server = async move {
            debug!("Source starting.");

            let mut result = select! {
                biased;

                // We've been told that we must forcefully shut down.
                _ = force_shutdown_tripwire => Ok(()),

                // The source pump encountered an error, which we're now bubbling up here to stop
                // the source as well, since the source running makes no sense without the pump.
                //
                // We only match receiving a message, not the error of the sender being dropped,
                // just to keep things simpler.
                Ok(e) = &mut pump_error_rx => Err(e),

                // The source finished normally.
                result = server => result.map_err(|_| TaskError::Opaque),
            };

            // Even though we already tried to receive any pump task error above, we may have exited
            // on the source itself returning an error due to task scheduling, where the pump task
            // encountered an error, sent it over the oneshot, but we were polling the source
            // already and hit an error trying to send to the now-shutdown pump task.
            //
            // Since the error from the source is opaque at the moment (i.e. `()`), we try a final
            // time to see if the pump task encountered an error, using _that_ instead if so, to
            // propagate the true error that caused the source to have to stop.
            if let Ok(e) = pump_error_rx.try_recv() {
                result = Err(e);
            }

            match result {
                Ok(()) => {
                    debug!("Source finished normally.");
                    Ok(TaskOutput::Source)
                }
                Err(e) => {
                    debug!("Source finished with an error.");
                    Err(e)
                }
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
        .transforms()
        .filter(|(key, _)| diff.transforms.contains_new(key))
    {
        debug!(component = %key, "Building new transform.");

        let mut schema_definitions = HashMap::new();
        let merged_definition =
            schema::merged_definition(&transform.inputs, config, &mut definition_cache);

        let span = error_span!(
            "transform",
            component_kind = "transform",
            component_id = %key.id(),
            component_type = %transform.inner.get_component_name(),
            // maintained for compatibility
            component_name = %key.id(),
        );

        for output in transform
            .inner
            .outputs(&merged_definition, config.schema.log_namespace())
        {
            let definition = output
                .log_schema_definition
                .unwrap_or_else(|| merged_definition.clone());
            schema_definitions.insert(output.port, definition);
        }

        let context = TransformContext {
            key: Some(key.clone()),
            globals: config.global.clone(),
            enrichment_tables: enrichment_tables.clone(),
            schema_definitions,
            merged_schema_definition: merged_definition.clone(),
            schema: config.schema,
        };

        let node = TransformNode::from_parts(
            key.clone(),
            transform,
            &merged_definition,
            config.schema.log_namespace(),
        );

        let transform = match transform
            .inner
            .build(&context)
            .instrument(span.clone())
            .await
        {
            Err(error) => {
                errors.push(format!("Transform \"{}\": {}", key, error));
                continue;
            }
            Ok(transform) => transform,
        };

        let (input_tx, input_rx) =
            TopologyBuilder::standalone_memory(TOPOLOGY_BUFFER_SIZE, WhenFull::Block).await;

        inputs.insert(key.clone(), (input_tx, node.inputs.clone()));

        let (transform_task, transform_outputs) = {
            let _span = span.enter();
            build_transform(transform, node, input_rx)
        };

        outputs.extend(transform_outputs);
        tasks.insert(key.clone(), transform_task);
    }

    // Build sinks
    for (key, sink) in config
        .sinks()
        .filter(|(key, _)| diff.sinks.contains_new(key))
    {
        debug!(component = %key, "Building new sink.");

        let sink_inputs = &sink.inputs;
        let healthcheck = sink.healthcheck();
        let enable_healthcheck = healthcheck.enabled && config.healthchecks.enabled;

        let typetag = sink.inner.get_component_name();
        let input_type = sink.inner.input().data_type();

        if config.schema.validation {
            // At this point, we've validated that all transforms are valid, including any
            // transform that mutates the schema provided by their sources. We can now validate the
            // schema expectations of each individual sink.
            if let Err(mut err) = schema::validate_sink_expectations(key, sink, config) {
                errors.append(&mut err);
            };
        }

        let (tx, rx) = if let Some(buffer) = buffers.remove(key) {
            buffer
        } else {
            let buffer_type = match sink.buffer.stages().first().expect("cant ever be empty") {
                BufferType::Memory { .. } => "memory",
                BufferType::DiskV2 { .. } => "disk",
            };
            let buffer_span = error_span!(
                "sink",
                component_kind = "sink",
                component_id = %key.id(),
                component_type = typetag,
                component_name = %key.id(),
                buffer_type,
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
                Ok((tx, rx)) => (tx, Arc::new(Mutex::new(Some(rx.into_stream())))),
            }
        };

        let cx = SinkContext {
            healthcheck,
            globals: config.global.clone(),
            proxy: ProxyConfig::merge_with_env(&config.global.proxy, sink.proxy()),
            schema: config.schema,
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
            debug!("Sink starting.");

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

            let mut rx = wrap(rx);

            let events_received = register!(EventsReceived);
            sink.run(
                rx.by_ref()
                    .filter(|events: &EventArray| ready(filter_events_type(events, input_type)))
                    .inspect(|events| {
                        events_received.emit(CountByteSize(
                            events.len(),
                            events.estimated_json_encoded_size_of(),
                        ))
                    })
                    .take_until_if(tripwire),
            )
            .await
            .map(|_| {
                debug!("Sink finished normally.");
                TaskOutput::Sink(rx)
            })
            .map_err(|_| {
                debug!("Sink finished with an error.");
                TaskError::Opaque
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
                            info!("Healthcheck passed.");
                            Ok(TaskOutput::Healthcheck)
                        }
                        Ok(Err(error)) => {
                            error!(
                                msg = "Healthcheck failed.",
                                %error,
                                component_kind = "sink",
                                component_type = typetag,
                                component_id = %component_key.id(),
                                // maintained for compatibility
                                component_name = %component_key.id(),
                            );
                            Err(TaskError::wrapped(error))
                        }
                        Err(e) => {
                            error!(
                                msg = "Healthcheck timed out.",
                                component_kind = "sink",
                                component_type = typetag,
                                component_id = %component_key.id(),
                                // maintained for compatibility
                                component_name = %component_key.id(),
                            );
                            Err(TaskError::wrapped(Box::new(e)))
                        }
                    })
                    .await
            } else {
                info!("Healthcheck disabled.");
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
    inputs: Inputs<OutputId>,
    input_details: Input,
    outputs: Vec<Output>,
    enable_concurrency: bool,
}

impl TransformNode {
    pub fn from_parts(
        key: ComponentKey,
        transform: &TransformOuter<OutputId>,
        schema_definition: &Definition,
        global_log_namespace: LogNamespace,
    ) -> Self {
        Self {
            key,
            typetag: transform.inner.get_component_name(),
            inputs: transform.inputs.clone(),
            input_details: transform.inner.input(),
            outputs: transform
                .inner
                .outputs(schema_definition, global_log_namespace),
            enable_concurrency: transform.inner.enable_concurrency(),
        }
    }
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

    let transform = async move {
        debug!("Synchronous transform starting.");

        match transform.await {
            Ok(v) => {
                debug!("Synchronous transform finished normally.");
                Ok(v)
            }
            Err(e) => {
                debug!("Synchronous transform finished with an error.");
                Err(e)
            }
        }
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
    events_received: Registered<EventsReceived>,
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
            events_received: register!(EventsReceived),
        }
    }

    fn on_events_received(&mut self, events: &EventArray) {
        let stopped = self.timer.stop_wait();
        if stopped.duration_since(self.last_report).as_secs() >= 5 {
            self.timer.report();
            self.last_report = stopped;
        }

        self.events_received.emit(CountByteSize(
            events.len(),
            events.estimated_json_encoded_size_of(),
        ));
    }

    async fn send_outputs(&mut self, outputs_buf: &mut TransformOutputsBuf) -> crate::Result<()> {
        self.timer.start_wait();
        self.outputs.send(outputs_buf).await
    }

    async fn run_inline(mut self) -> TaskResult {
        // 128 is an arbitrary, smallish constant
        const INLINE_BATCH_SIZE: usize = 128;

        let mut outputs_buf = self.outputs.new_buf_with_capacity(INLINE_BATCH_SIZE);

        let mut input_rx = self
            .input_rx
            .take()
            .expect("can't run runner twice")
            .into_stream()
            .filter(move |events| ready(filter_events_type(events, self.input_type)));

        self.timer.start_wait();
        while let Some(events) = input_rx.next().await {
            self.on_events_received(&events);
            self.transform.transform_all(events, &mut outputs_buf);
            self.send_outputs(&mut outputs_buf)
                .await
                .map_err(TaskError::wrapped)?;
        }

        Ok(TaskOutput::Transform)
    }

    async fn run_concurrently(mut self) -> TaskResult {
        let input_rx = self
            .input_rx
            .take()
            .expect("can't run runner twice")
            .into_stream()
            .filter(move |events| ready(filter_events_type(events, self.input_type)));

        let mut input_rx =
            super::ready_arrays::ReadyArrays::with_capacity(input_rx, READY_ARRAY_CAPACITY);

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
                            self.send_outputs(&mut outputs_buf).await
                                .map_err(TaskError::wrapped)?;
                        }
                        _ => unreachable!("join error or bad poll"),
                    }
                }

                input_arrays = input_rx.next(), if in_flight.len() < *TRANSFORM_CONCURRENCY_LIMIT && !shutting_down => {
                    match input_arrays {
                        Some(input_arrays) => {
                            let mut len = 0;
                            for events in &input_arrays {
                                self.on_events_received(events);
                                len += events.len();
                            }

                            let mut t = self.transform.clone();
                            let mut outputs_buf = self.outputs.new_buf_with_capacity(len);
                            let task = tokio::spawn(async move {
                                for events in input_arrays {
                                    t.transform_all(events, &mut outputs_buf);
                                }
                                outputs_buf
                            }.in_current_span());
                            in_flight.push_back(task);
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

    let input_rx = crate::utilization::wrap(input_rx.into_stream());

    let events_received = register!(EventsReceived);
    let filtered = input_rx
        .filter(move |events| ready(filter_events_type(events, input_type)))
        .inspect(move |events| {
            events_received.emit(CountByteSize(
                events.len(),
                events.estimated_json_encoded_size_of(),
            ))
        });
    let events_sent = register!(EventsSent::from(internal_event::Output(None)));
    let stream = t
        .transform(Box::pin(filtered))
        .inspect(move |events: &EventArray| {
            events_sent.emit(CountByteSize(
                events.len(),
                events.estimated_json_encoded_size_of(),
            ));
        });
    let transform = async move {
        debug!("Task transform starting.");

        match fanout.send_stream(stream).await {
            Ok(()) => {
                debug!("Task transform finished normally.");
                Ok(TaskOutput::Transform)
            }
            Err(e) => {
                debug!("Task transform finished with an error.");
                Err(TaskError::wrapped(e))
            }
        }
    }
    .boxed();

    let mut outputs = HashMap::new();
    outputs.insert(OutputId::from(key), control);

    let task = Task::new(key.clone(), typetag, transform);

    (task, outputs)
}
