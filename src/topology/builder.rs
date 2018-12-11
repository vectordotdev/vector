use super::config;
use futures::prelude::*;
use futures::{future, sync::mpsc, Future};
use log::error;
use regex::{Regex, RegexSet};
use sinks;
use sources;
use std::collections::HashMap;
use stream_cancel::{Trigger, Tripwire};
use transforms;
use Record;

pub fn build(config: super::Config) -> (impl Future<Item = (), Error = ()>, Trigger) {
    let (trigger, tripwire) = Tripwire::new();

    let lazy = future::lazy(move || {
        // Maps the name of an upstream component to the input channels of its
        // downstream components.
        let mut connections: HashMap<String, sinks::RouterSink> = HashMap::new();

        // TODO: NLL might let us remove this extra block
        let transform_rxs;
        {
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
            for (_name, sink) in config.sinks.into_iter() {
                let rx = add_connections(sink.inputs);

                let sink_task = build_sink(sink.inner)
                    .map_err(|e| error!("error creating sender: {:?}", e))
                    .and_then(|sink| rx.forward(sink).map(|_| ()));

                tokio::spawn(sink_task);
            }

            // For each transform, set up an inbound channel (like the sinks above).
            transform_rxs = config
                .transforms
                .into_iter()
                .map(|(name, outer)| {
                    let rx = add_connections(outer.inputs);

                    (name, outer.inner, rx)
                })
                .collect::<Vec<_>>();
        }

        // For each transform, spawn a task that reads from its inbound channel,
        // transforms the record, and then sends the transformed record to each downstream
        // component.
        // This needs to be a separate loop from the one above to make sure that all of the
        // connection outputs are set up before the inputs start using them.
        for (name, transform, rx) in transform_rxs.into_iter() {
            let transform = build_transform(transform);
            let outputs = connections.remove(&name).unwrap();
            let transform_task = rx
                .filter_map(move |r| transform.transform(r))
                .forward(outputs)
                .map(|_| ());
            tokio::spawn(transform_task);
        }

        // For each source, set up a channel to aggregate all of its handlers together,
        // spin up a task to pump from that channel to each of the downstream channels,
        // and start the listener task.
        for (name, source) in config.sources {
            let (tx, rx) = futures::sync::mpsc::channel(1000);

            let outputs = connections.remove(&name).unwrap();
            let pump_task = rx.forward(outputs).map(|_| ());
            tokio::spawn(pump_task);

            let server = build_source(source, tx);
            let server = server.select(tripwire.clone()).map(|_| ()).map_err(|_| ());
            tokio::spawn(server);
        }

        future::ok(())
    });

    (lazy, trigger)
}

fn build_sink(sink: config::Sink) -> sinks::RouterSinkFuture {
    match sink {
        config::Sink::Splunk { address } => sinks::splunk::raw_tcp(address),
        config::Sink::Elasticsearch => sinks::elasticsearch::ElasticseachSink::build(),
    }
}

fn build_source(source: config::Source, out: mpsc::Sender<Record>) -> sources::Source {
    match source {
        config::Source::Splunk { address } => sources::splunk::raw_tcp(address, out),
    }
}

fn build_transform(transform: config::Transform) -> Box<transforms::Transform> {
    match transform {
        config::Transform::Sampler { rate, pass_list } => Box::new(transforms::Sampler::new(
            rate,
            RegexSet::new(pass_list).unwrap(),
        )),
        config::Transform::RegexParser { regex } => {
            Box::new(transforms::RegexParser::new(Regex::new(&regex).unwrap()))
        }
        config::Transform::FieldFilter { field, value } => {
            Box::new(transforms::FieldFilter::new(field, value))
        }
    }
}
