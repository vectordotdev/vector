use futures::prelude::*;
use futures::{future, Future};
use log::error;
use sinks;
use sources;
use std::collections::HashMap;
use stream_cancel::{Trigger, Tripwire};
use transforms;
use Record;

pub struct TopologyBuilder {
    sources: HashMap<String, sources::Source>,
    sinks: HashMap<String, sinks::RouterSinkFuture>,
    transforms: HashMap<String, Box<dyn transforms::Transform>>,
    connections: HashMap<String, Vec<String>>,
    tripwire: Tripwire,
    trigger: Trigger,
}

// TODO: Better error handling
impl TopologyBuilder {
    pub fn new() -> Self {
        let (trigger, tripwire) = Tripwire::new();

        Self {
            sources: HashMap::new(),
            sinks: HashMap::new(),
            transforms: HashMap::new(),
            connections: HashMap::new(),
            trigger,
            tripwire,
        }
    }

    pub fn add_source<Factory: sources::SourceFactory>(
        &mut self,
        config: Factory::Config,
        name: &str,
    ) {
        let source = Factory::build(config, self.tripwire.clone());
        let existing = self.sources.insert(name.to_owned(), source);
        if existing.is_some() {
            panic!("Multiple sources with same name: {}", name);
        }
    }

    pub fn add_sink<Factory: sinks::SinkFactory>(&mut self, config: Factory::Config, name: &str) {
        let sink = Factory::build(config);
        let existing = self.sinks.insert(name.to_owned(), sink);
        if existing.is_some() {
            panic!("Multiple sinks with same name: {}", name);
        }
    }

    pub fn add_transform<Factory: transforms::TransformFactory>(
        &mut self,
        config: Factory::Config,
        name: &str,
    ) {
        let transform = Factory::build(config);
        let existing = self.transforms.insert(name.to_owned(), transform);
        // TODO: need to ensure this also doesn't overlap with sources or sinks
        if existing.is_some() {
            panic!("Multiple transforms with same name: {}", name);
        }
    }

    pub fn connect(&mut self, in_name: &str, out_name: &str) {
        self.connections
            .entry(in_name.to_owned())
            .or_insert_with(Vec::new)
            .push(out_name.to_owned());
    }

    // Each sink sets up a multi-producer channel that it reads from. Each source writes to the channel
    // for each of the sinks it's connected to. Transforms work like a combination source/sink; they have
    // a channel for accepting records, which are then transformed and sent downstream to the connected sinks.
    // All of these are joined into a single future that drives work on the entire topology.
    pub fn build(self) -> (impl Future<Item = (), Error = ()>, Trigger) {
        let TopologyBuilder {
            trigger,
            sinks,
            sources,
            transforms,
            mut connections,
            ..
        } = self;

        let lazy = future::lazy(move || {
            let mut txs = HashMap::new();

            let sink_map_err = |e| error!("sender error: {:?}", e);

            for (name, sink) in sinks.into_iter() {
                let (tx, rx) = futures::sync::mpsc::channel(0);

                txs.insert(name, tx.sink_map_err(sink_map_err));

                let sink_task = sink
                    .map_err(|e| error!("error creating sender: {:?}", e))
                    .and_then(|sink| rx.forward(sink).map(|_| ()));

                tokio::spawn(sink_task);
            }

            let mut transform_rxs = vec![];

            for (name, transform) in transforms.into_iter() {
                let (tx, rx) = futures::sync::mpsc::channel(0);

                txs.insert(name.clone(), tx.sink_map_err(sink_map_err));

                let rx = rx.filter_map(move |r| transform.transform(r));
                transform_rxs.push((name, rx));
            }

            let mut outs = |name| -> Box<dyn Sink<SinkItem = Record, SinkError = ()> + Send> {
                let out_names = connections.remove(&name).unwrap();
                let mut outs = out_names
                    .into_iter()
                    .map(|out_name| Box::new(txs[&out_name].clone()));
                let first_out = outs.next().unwrap();
                let out: Box<dyn Sink<SinkItem = Record, SinkError = ()> + Send> =
                    outs.fold(first_out, |a, b| Box::new(a.fanout(b)));

                out
            };

            for (name, rx) in transform_rxs.into_iter() {
                let transform_task = rx.forward(outs(name)).map(|_| ());
                tokio::spawn(transform_task);
            }

            for (name, stream) in sources.into_iter() {
                let source_task = stream.forward(outs(name)).map(|_| ());
                tokio::spawn(source_task);
            }
            future::ok(())
        });

        (lazy, trigger)
    }
}
