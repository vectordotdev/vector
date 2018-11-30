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
        let mut connections = HashMap::new();

        fn add_connections(
            connections: &mut HashMap<String, sinks::RouterSink>,
            inputs: Vec<String>,
        ) -> mpsc::Receiver<Record> {
            let (tx, rx) = futures::sync::mpsc::channel(0);
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
        }

        for (_name, sink) in config.sinks.into_iter() {
            let rx = add_connections(&mut connections, sink.inputs);

            let sink_task = build_sink(sink.inner)
                .map_err(|e| error!("error creating sender: {:?}", e))
                .and_then(|sink| rx.forward(sink).map(|_| ()));

            tokio::spawn(sink_task);
        }

        // We have to iterator over the transforms twice, once to set up their inputs
        // and then again to connect them to their outputs
        let transform_rxs = config
            .transforms
            .into_iter()
            .map(|(name, outer)| {
                let transform = build_transform(outer.inner);

                let rx = add_connections(&mut connections, outer.inputs)
                    .filter_map(move |r| transform.transform(r));

                (name, rx)
            })
            .collect::<Vec<_>>();

        for (name, rx) in transform_rxs.into_iter() {
            let outputs = connections.remove(&name).unwrap();
            let transform_task = rx.forward(outputs).map(|_| ());
            tokio::spawn(transform_task);
        }

        let servers = config.sources.into_iter().map(|(name, source)| {
            let (tx, rx) = futures::sync::mpsc::channel(1000);

            let outputs = connections.remove(&name).unwrap();
            let pump_task = rx.forward(outputs).map(|_| ());
            tokio::spawn(pump_task);

            build_source(source, tx)
        });

        for server in servers {
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
