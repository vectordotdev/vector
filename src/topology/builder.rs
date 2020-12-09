use super::{
    fanout::{self, Fanout},
    task::{Task, TaskOutput},
    BuiltBuffer, ConfigDiff,
};
use crate::{
    buffers,
    config::{DataType, SinkContext},
    event::Event,
    shutdown::SourceShutdownCoordinator,
    transforms::Transform,
    Pipeline,
};
use futures::{
    compat::{Future01CompatExt, Sink01CompatExt, Stream01CompatExt},
    future, FutureExt, StreamExt, TryFutureExt,
};
use futures01::{Future as Future01, Stream as Stream01};
use std::{
    collections::HashMap,
    future::ready,
    sync::{Arc, Mutex},
};
use stream_cancel::{StreamExt as StreamCancelExt, Trigger, Tripwire};
use tokio::{
    sync::mpsc,
    time::{timeout, Duration},
};

pub struct Pieces {
    pub inputs: HashMap<String, (buffers::BufferInputCloner, Vec<String>)>,
    pub outputs: HashMap<String, fanout::ControlChannel>,
    pub tasks: HashMap<String, Task>,
    pub source_tasks: HashMap<String, Task>,
    pub healthchecks: HashMap<String, Task>,
    pub shutdown_coordinator: SourceShutdownCoordinator,
    pub detach_triggers: HashMap<String, Trigger>,
}

/// Builds only the new pieces, and doesn't check their topology.
pub async fn build_pieces(
    config: &super::Config,
    diff: &ConfigDiff,
    mut buffers: HashMap<String, BuiltBuffer>,
) -> Result<Pieces, Vec<String>> {
    let mut inputs = HashMap::new();
    let mut outputs = HashMap::new();
    let mut tasks = HashMap::new();
    let mut source_tasks = HashMap::new();
    let mut healthchecks = HashMap::new();
    let mut shutdown_coordinator = SourceShutdownCoordinator::default();
    let mut detach_triggers = HashMap::new();

    let mut errors = vec![];

    // Build sources
    for (name, source) in config
        .sources
        .iter()
        .filter(|(name, _)| diff.sources.contains_new(&name))
    {
        let (tx, rx) = mpsc::channel(1000);
        let pipeline = Pipeline::from_sender(tx, vec![]);

        let typetag = source.source_type();

        let (shutdown_signal, force_shutdown_tripwire) = shutdown_coordinator.register_source(name);

        let server = match source
            .build(&name, &config.global, shutdown_signal, pipeline)
            .await
        {
            Err(error) => {
                errors.push(format!("Source \"{}\": {}", name, error));
                continue;
            }
            Ok(server) => server,
        };

        let (output, control) = Fanout::new();
        let pump = rx
            .map(Ok)
            .forward(output.sink_compat())
            .map_ok(|_| TaskOutput::Source);
        let pump = Task::new(name, typetag, pump);

        // The force_shutdown_tripwire is a Future that when it resolves means that this source
        // has failed to shut down gracefully within its allotted time window and instead should be
        // forcibly shut down. We accomplish this by select()-ing on the server Task with the
        // force_shutdown_tripwire. That means that if the force_shutdown_tripwire resolves while
        // the server Task is still running the Task will simply be dropped on the floor.
        let server = future::try_select(server, force_shutdown_tripwire.unit_error().boxed())
            .map_ok(|_| {
                debug!("Finished.");
                TaskOutput::Source
            })
            .map_err(|_| ());
        let server = Task::new(name, typetag, server);

        outputs.insert(name.clone(), control);
        tasks.insert(name.clone(), pump);
        source_tasks.insert(name.clone(), server);
    }

    // Build transforms
    for (name, transform) in config
        .transforms
        .iter()
        .filter(|(name, _)| diff.transforms.contains_new(&name))
    {
        let trans_inputs = &transform.inputs;

        let typetag = transform.inner.transform_type();

        let input_type = transform.inner.input_type();
        let transform = match transform.inner.build().await {
            Err(error) => {
                errors.push(format!("Transform \"{}\": {}", name, error));
                continue;
            }
            Ok(transform) => transform,
        };

        let (input_tx, input_rx) = futures01::sync::mpsc::channel(100);
        let input_tx = buffers::BufferInputCloner::Memory(input_tx, buffers::WhenFull::Block);

        let (output, control) = Fanout::new();

        let transform = match transform {
            Transform::Function(mut t) => {
                let filtered = filter_event_type(input_rx, input_type);
                #[allow(deprecated)]
                // `boxed()` here is deprecated, but the replacement won't work until we adopt futures 0.3 here.
                let transformed = filtered
                    .map(move |v| {
                        let mut buf = Vec::with_capacity(1);
                        t.transform(&mut buf, v);
                        futures01::stream::iter_ok(buf.into_iter())
                    })
                    .flatten()
                    .boxed();
                transformed.forward(output)
            }
            Transform::Task(t) => {
                let filtered = filter_event_type(input_rx, input_type);
                let transformed: Box<dyn futures01::Stream<Item = _, Error = _> + Send> =
                    t.transform(filtered);
                transformed.forward(output)
            }
        }
        .map(|_| {
            debug!("Finished.");
            TaskOutput::Transform
        })
        .compat();
        let task = Task::new(name, typetag, transform);

        inputs.insert(name.clone(), (input_tx, trans_inputs.clone()));
        outputs.insert(name.clone(), control);
        tasks.insert(name.clone(), task);
    }

    // Build sinks
    for (name, sink) in config
        .sinks
        .iter()
        .filter(|(name, _)| diff.sinks.contains_new(&name))
    {
        let sink_inputs = &sink.inputs;
        let enable_healthcheck = sink.healthcheck;

        let typetag = sink.inner.sink_type();
        let input_type = sink.inner.input_type();

        let (tx, rx, acker) = if let Some(buffer) = buffers.remove(name) {
            buffer
        } else {
            let buffer = sink.buffer.build(&config.global.data_dir, &name);
            match buffer {
                Err(error) => {
                    errors.push(format!("Sink \"{}\": {}", name, error));
                    continue;
                }
                Ok((tx, rx, acker)) => (tx, Arc::new(Mutex::new(Some(rx))), acker),
            }
        };

        let cx = SinkContext {
            acker: acker.clone(),
        };

        let (sink, healthcheck) = match sink.inner.build(cx).await {
            Err(error) => {
                errors.push(format!("Sink \"{}\": {}", name, error));
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
            let mut rx = rx
                .lock()
                .unwrap()
                .take()
                .expect("Task started but input has been taken.");

            sink.run(
                (&mut rx)
                    .filter(|event| match input_type {
                        DataType::Any => true,
                        DataType::Log => matches!(event, Event::Log(_)),
                        DataType::Metric => matches!(event, Event::Metric(_)),
                    })
                    .compat()
                    .take_while(|e| ready(e.is_ok()))
                    .take_until_if(tripwire)
                    .map(|x| x.unwrap()),
            )
            .await
            .map(|_| {
                debug!("Finished.");
                TaskOutput::Sink(rx, acker)
            })
        };
        let task = Task::new(name, typetag, sink);

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
                            error!(message = "Healthcheck: Failed Reason.", %error);
                            Err(())
                        }
                        Err(_) => {
                            error!("Healthcheck: timeout.");
                            Err(())
                        }
                    })
                    .await
            } else {
                info!("Healthcheck: Disabled.");
                Ok(TaskOutput::Healthcheck)
            }
        };
        let healthcheck_task = Task::new(name, typetag, healthcheck_task);

        inputs.insert(name.clone(), (tx, sink_inputs.clone()));
        healthchecks.insert(name.clone(), healthcheck_task);
        tasks.insert(name.clone(), task);
        detach_triggers.insert(name.clone(), trigger);
    }

    if errors.is_empty() {
        let pieces = Pieces {
            inputs,
            outputs,
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

fn filter_event_type<S>(
    stream: S,
    data_type: DataType,
) -> Box<dyn Stream01<Item = Event, Error = ()> + Send>
where
    S: Stream01<Item = Event, Error = ()> + Send + 'static,
{
    match data_type {
        DataType::Any => Box::new(stream), // it's possible to not call any comparing function if any type is supported
        DataType::Log => Box::new(stream.filter(|event| matches!(event, Event::Log(_)))),
        DataType::Metric => Box::new(stream.filter(|event| matches!(event, Event::Metric(_)))),
    }
}
