use super::config;
use crate::{record::Record, sinks, sources, transforms};
use futures::prelude::*;
use futures::{future, sync::mpsc, Future};
use log::error;
use regex::{Regex, RegexSet};
use std::collections::HashMap;
use stream_cancel::{Trigger, Tripwire};

pub fn build(
    config: super::Config,
) -> Result<(impl Future<Item = (), Error = ()>, Trigger, Vec<String>), Vec<String>> {
    let mut tasks: Vec<Box<dyn Future<Item = (), Error = ()> + Send>> = vec![];
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
    let mut add_connections = |inputs: Vec<String>| -> mpsc::Receiver<Record> {
        let (tx, rx) = futures::sync::mpsc::channel(100);
        let tx = tx.sink_map_err(|e| error!("sender error: {:?}", e));

        for input in inputs {
            if let Some(existing) = connections.remove(&input) {
                let new = existing.fanout(tx.clone());
                connections.insert(input, Box::new(new));
            } else {
                connections.insert(input, Box::new(tx.clone()));
            }
        }

        rx
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
        let rx = add_connections(sink.inputs);

        match build_sink(sink.inner) {
            Err(error) => {
                errors.push(format!("Sink \"{}\": {}", name, error));
            }
            Ok(sink) => {
                let sink_task = sink
                    .map_err(|e| error!("error creating sender: {:?}", e))
                    .and_then(|sink| rx.forward(sink).map(|_| ()));

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
            let rx = add_connections(outer.inputs);

            (name, outer.inner, rx)
        })
        .collect::<Vec<_>>();

    // For each transform, spawn a task that reads from its inbound channel,
    // transforms the record, and then sends the transformed record to each downstream
    // component.
    // This needs to be a separate loop from the one above to make sure that all of the
    // connection outputs are set up before the inputs start using them.
    for (name, transform, rx) in transform_rxs.into_iter() {
        match build_transform(transform) {
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

        match build_source(source, tx) {
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

        Ok((lazy, trigger, warnings))
    } else {
        Err(errors)
    }
}

fn build_sink(sink: config::Sink) -> Result<sinks::RouterSinkFuture, String> {
    match sink {
        config::Sink::SplunkTcp { address } => Ok(sinks::splunk::raw_tcp(address)),
        config::Sink::SplunkHec { token, host } => Ok(sinks::splunk::hec(token, host)),
        config::Sink::Elasticsearch => Ok(sinks::elasticsearch::ElasticsearchSink::build()),
        config::Sink::S3 {
            bucket,
            key_prefix,
            region,
            endpoint,
            buffer_size,
            gzip,
        } => {
            use rusoto_core::region::Region;
            use rusoto_s3::S3Client;

            let region = if region.is_some() && endpoint.is_some() {
                return Err("Only one of 'region' or 'endpoint' can be specified".to_string());
            } else if let Some(region) = region {
                region.parse::<Region>().map_err(|e| e.to_string())?
            } else if let Some(endpoint) = endpoint {
                Region::Custom {
                    name: "custom".to_owned(),
                    endpoint: endpoint,
                }
            } else {
                return Err("Must set 'region' or 'endpoint'".to_string());
            };

            let client = S3Client::new(region);

            let config = sinks::s3::S3SinkConfig {
                client,
                gzip,
                buffer_size,
                key_prefix,
                bucket,
            };

            Ok(sinks::s3::new(config))
        }
    }
}

fn build_source(
    source: config::Source,
    out: mpsc::Sender<Record>,
) -> Result<sources::Source, String> {
    match source {
        config::Source::Splunk { address } => Ok(sources::splunk::raw_tcp(address, out)),
    }
}

fn build_transform(transform: config::Transform) -> Result<Box<dyn transforms::Transform>, String> {
    match transform {
        config::Transform::Sampler { rate, pass_list } => RegexSet::new(pass_list)
            .map_err(|err| err.to_string())
            .map::<Box<dyn transforms::Transform>, _>(|regex_set| {
                Box::new(transforms::Sampler::new(rate, regex_set))
            }),
        config::Transform::RegexParser { regex } => Regex::new(&regex)
            .map_err(|err| err.to_string())
            .map::<Box<dyn transforms::Transform>, _>(|r| {
                Box::new(transforms::RegexParser::new(r))
            }),
        config::Transform::FieldFilter { field, value } => {
            Ok(Box::new(transforms::FieldFilter::new(field, value)))
        }
    }
}
