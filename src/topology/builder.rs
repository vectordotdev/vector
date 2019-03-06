use super::fanout::{self, Fanout};
use crate::buffers;
use futures::prelude::*;
use futures::{sync::mpsc, Future};
use log::{error, info};
use std::collections::HashMap;
use stream_cancel::{Trigger, Tripwire};

type Task = Box<dyn Future<Item = (), Error = ()> + Send>;

pub struct Pieces {
    pub inputs: HashMap<String, (buffers::BufferInputCloner, Vec<String>)>,
    pub outputs: HashMap<String, fanout::ControlChannel>,
    pub tasks: HashMap<String, Vec<Task>>,
    pub healthchecks: HashMap<String, Task>,
    pub shutdown_triggers: HashMap<String, Trigger>,
}

pub fn build_pieces(config: &super::Config) -> Result<(Pieces, Vec<String>), Vec<String>> {
    let mut inputs = HashMap::new();
    let mut outputs = HashMap::new();
    let mut tasks = HashMap::new();
    let mut healthchecks = HashMap::new();
    let mut shutdown_triggers = HashMap::new();

    let mut errors = vec![];
    let mut warnings = vec![];

    // Build sources
    for (name, source) in &config.sources {
        let (tx, rx) = mpsc::channel(1000);

        let server = match source.build(tx) {
            Err(error) => {
                errors.push(format!("Transform \"{}\": {}", name, error));
                continue;
            }
            Ok(server) => server,
        };

        let (trigger, tripwire) = Tripwire::new();

        let (output, control) = Fanout::new();
        let pump = rx.forward(output).map(|_| ());
        let pump: Task = Box::new(pump);

        let server = server.select(tripwire.clone()).map(|_| ()).map_err(|_| ());
        let server: Task = Box::new(server);

        outputs.insert(name.clone(), control);
        tasks.insert(name.clone(), vec![pump, server]);
        shutdown_triggers.insert(name.clone(), trigger);
    }

    // Build transforms
    for (name, transform) in &config.transforms {
        let trans_inputs = &transform.inputs;
        let transform = match transform.inner.build() {
            Err(error) => {
                errors.push(format!("Transform \"{}\": {}", name, error));
                continue;
            }
            Ok(transform) => transform,
        };

        let (input_tx, input_rx) = futures::sync::mpsc::channel(100);
        let input_tx = buffers::BufferInputCloner::Memory(input_tx);

        let (output, control) = Fanout::new();

        let task = input_rx
            .filter_map(move |r| transform.transform(r))
            .forward(output)
            .map(|_| ());
        let task: Task = Box::new(task);

        inputs.insert(name.clone(), (input_tx, trans_inputs.clone()));
        outputs.insert(name.clone(), control);
        tasks.insert(name.clone(), vec![task]);
    }

    // Build sinks
    for (name, sink) in &config.sinks {
        let sink_inputs = &sink.inputs;

        let buffer = sink.buffer.build(&config.data_dir, &name);
        let (tx, rx) = match buffer {
            Err(error) => {
                errors.push(format!("Sink \"{}\": {}", name, error));
                continue;
            }
            Ok(buffer) => buffer,
        };

        let (sink, healthcheck) = match sink.inner.build() {
            Err(error) => {
                errors.push(format!("Sink \"{}\": {}", name, error));
                continue;
            }
            Ok((sink, healthcheck)) => (sink, healthcheck),
        };

        let task = rx.forward(sink).map(|_| ());
        let task: Task = Box::new(task);

        let name2 = name.clone();
        let name3 = name.clone();
        let healthcheck_task = healthcheck
            .map(move |_| info!("Healthcheck for {}: Ok", name2))
            .map_err(move |err| error!("Healthcheck for {}: ERROR: {}", name3, err));
        let healthcheck_task: Task = Box::new(healthcheck_task);

        inputs.insert(name.clone(), (tx, sink_inputs.clone()));
        healthchecks.insert(name.clone(), healthcheck_task);
        tasks.insert(name.clone(), vec![task]);
    }

    // Warnings and errors
    let sink_inputs = config
        .sinks
        .iter()
        .map(|(name, sink)| ("sink", name.clone(), sink.inputs.clone()));
    let transform_inputs = config
        .transforms
        .iter()
        .map(|(name, transform)| ("transform", name.clone(), transform.inputs.clone()));
    for (output_type, name, inputs) in sink_inputs.chain(transform_inputs) {
        if inputs.is_empty() {
            warnings.push(format!(
                "{} {:?} has no inputs",
                capitalize(output_type),
                name
            ));
        }

        for input in inputs {
            if !config.sources.contains_key(&input) && !config.transforms.contains_key(&input) {
                errors.push(format!(
                    "Input {:?} for {} {:?} doesn't exist.",
                    input, output_type, name
                ));
            }
        }
    }

    let source_names = config.sources.keys().map(|name| ("source", name.clone()));
    let transform_names = config
        .transforms
        .keys()
        .map(|name| ("transform", name.clone()));
    for (input_type, name) in transform_names.chain(source_names) {
        if !config
            .transforms
            .iter()
            .any(|(_, transform)| transform.inputs.contains(&name))
            && !config
                .sinks
                .iter()
                .any(|(_, sink)| sink.inputs.contains(&name))
        {
            warnings.push(format!(
                "{} {:?} has no outputs",
                capitalize(input_type),
                name
            ));
        }
    }

    if errors.is_empty() {
        let pieces = Pieces {
            inputs,
            outputs,
            tasks,
            healthchecks,
            shutdown_triggers,
        };

        Ok((pieces, warnings))
    } else {
        Err(errors)
    }
}

fn capitalize(s: &str) -> String {
    let mut s = s.to_owned();
    if let Some(r) = s.get_mut(0..1) {
        r.make_ascii_uppercase();
    }
    s
}
