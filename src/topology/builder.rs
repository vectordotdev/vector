use crate::{buffers, sinks};
use futures::prelude::*;
use futures::{future, Future};
use log::{error, info};
use std::collections::HashMap;
use stream_cancel::{Trigger, Tripwire};

pub fn build(
    config: super::Config,
) -> Result<
    (
        impl Future<Item = (), Error = ()>,
        Trigger,
        impl Future<Item = (), Error = ()>,
        Vec<String>,
    ),
    Vec<String>,
> {
    let mut tasks: Vec<Box<dyn Future<Item = (), Error = ()> + Send>> = vec![];
    let mut healthcheck_tasks = vec![];
    let mut errors = vec![];
    let mut warnings = vec![];

    let (trigger, tripwire) = Tripwire::new();

    // Maps the name of an upstream component to the input channels of its
    // downstream components.
    let mut connections: HashMap<String, sinks::RouterSink> = HashMap::new();

    let mut input_names = vec![];
    input_names.extend(config.sources.keys().cloned());
    input_names.extend(config.transforms.keys().cloned());

    // Creates a channel for a downstream component, and adds it to the set
    // of outbound channels for each of its inputs.
    let mut add_connections = |inputs: Vec<String>, bic: buffers::BufferInputCloner| {
        for input in inputs {
            if let Some(existing) = connections.remove(&input) {
                let new = existing.fanout(bic.get());
                connections.insert(input, Box::new(new));
            } else {
                connections.insert(input, bic.get());
            }
        }
    };

    // For each sink, set up its inbound channel and spawn a task that pumps
    // from that channel into the sink.
    for (name, sink) in config.sinks.into_iter() {
        for input in &sink.inputs {
            if !input_names.contains(&input) {
                errors.push(format!(
                    "Input \"{}\" for sink \"{}\" doesn't exist.",
                    input, name
                ));
            }
        }
        if sink.inputs.is_empty() {
            warnings.push(format!("Sink \"{}\" has no inputs", name));
        }

        let buffer = sink.buffer.build(&config.data_dir, &name);

        let (tx, rx) = match buffer {
            Err(error) => {
                errors.push(format!("Sink \"{}\": {}", name, error));
                continue;
            }
            Ok(buffer) => buffer,
        };

        add_connections(sink.inputs, tx);

        match sink.inner.build() {
            Err(error) => {
                errors.push(format!("Sink \"{}\": {}", name, error));
            }
            Ok((sink, healthcheck)) => {
                let name2 = name.clone();
                let healthcheck_task = healthcheck
                    .map(move |_| info!("Healthcheck for {}: Ok", name))
                    .map_err(move |err| error!("Healthcheck for {}: ERROR: {}", name2, err));
                healthcheck_tasks.push(healthcheck_task);

                let sink_task = rx.forward(sink).map(|_| ());

                tasks.push(Box::new(sink_task));
            }
        }
    }

    // For each transform, set up an inbound channel (like the sinks above).
    let transform_rxs = config
        .transforms
        .into_iter()
        .map(|(name, outer)| {
            for input in &outer.inputs {
                if !input_names.contains(&input) {
                    errors.push(format!(
                        "Input \"{}\" for transform \"{}\" doesn't exist.",
                        input, name
                    ));
                }
            }
            if outer.inputs.is_empty() {
                warnings.push(format!("Transform \"{}\" has no inputs", name));
            }
            let (tx, rx) = futures::sync::mpsc::channel(100);
            add_connections(outer.inputs, buffers::BufferInputCloner::Memory(tx));

            (name, outer.inner, rx)
        })
        .collect::<Vec<_>>();

    // For each transform, spawn a task that reads from its inbound channel,
    // transforms the record, and then sends the transformed record to each downstream
    // component.
    // This needs to be a separate loop from the one above to make sure that all of the
    // connection outputs are set up before the inputs start using them.
    for (name, transform, rx) in transform_rxs.into_iter() {
        match transform.build() {
            Err(error) => {
                errors.push(format!("Transform \"{}\": {}", name, error));
            }
            Ok(transform) => {
                let outputs = connections.remove(&name).unwrap_or_else(|| {
                    warnings.push(format!("Transform \"{}\" has no outputs", name));
                    Box::new(crate::sinks::BlackHole)
                });
                let transform_task = rx
                    .filter_map(move |r| transform.transform(r))
                    .forward(outputs)
                    .map(|_| ());
                tasks.push(Box::new(transform_task));
            }
        }
    }

    // For each source, set up a channel to aggregate all of its handlers together,
    // spin up a task to pump from that channel to each of the downstream channels,
    // and start the listener task.
    for (name, source) in config.sources {
        let (tx, rx) = futures::sync::mpsc::channel(1000);

        let outputs = connections.remove(&name).unwrap_or_else(|| {
            warnings.push(format!("Source \"{}\" has no outputs", name));
            Box::new(crate::sinks::BlackHole)
        });
        let pump_task = rx.forward(outputs).map(|_| ());
        tasks.push(Box::new(pump_task));

        match source.build(tx) {
            Err(error) => {
                errors.push(format!("Transform \"{}\": {}", name, error));
            }
            Ok(server) => {
                let server = server.select(tripwire.clone()).map(|_| ()).map_err(|_| ());
                tasks.push(Box::new(server));
            }
        }
    }

    if errors.is_empty() {
        let lazy = future::lazy(move || {
            for task in tasks {
                tokio::spawn(task);
            }

            future::ok(())
        });

        let healthchecks = futures::future::join_all(healthcheck_tasks).map(|_| ());

        Ok((lazy, trigger, healthchecks, warnings))
    } else {
        Err(errors)
    }
}
