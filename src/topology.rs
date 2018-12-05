use futures::prelude::*;
use futures::{future, sync::mpsc, Future};
use log::error;
use sinks;
use sources;
use std::collections::HashMap;
use stream_cancel::{Trigger, Tripwire};
use transforms;
use Record;

pub struct TopologyBuilder {
    sources: HashMap<
        String,
        Box<dyn Fn(mpsc::Sender<Record>) -> Box<dyn Future<Item = (), Error = ()> + Send> + Send>,
    >,
    sinks: HashMap<String, sinks::RouterSinkFuture>,
    transforms: HashMap<String, Box<dyn transforms::Transform>>,
    connections: HashMap<String, Vec<String>>,
}

// TODO: Better error handling
impl TopologyBuilder {
    pub fn new() -> Self {
        Self {
            sources: HashMap::new(),
            sinks: HashMap::new(),
            transforms: HashMap::new(),
            connections: HashMap::new(),
        }
    }

    pub fn add_source<Factory: sources::SourceFactory + 'static>(
        &mut self,
        config: Factory::Config,
        name: &str,
    ) {
        let curry = move |out: mpsc::Sender<Record>| Factory::build(config.clone(), out);

        let existing = self.sources.insert(name.to_owned(), Box::new(curry));
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
    pub fn build(self) -> (impl Future<Item = (), Error = ()>, Trigger) {
        let (trigger, tripwire) = Tripwire::new();

        let lazy = future::lazy(move || {
            let mut txs = HashMap::new();

            let sink_map_err = |e| error!("sender error: {:?}", e);

            for (name, sink) in self.sinks.into_iter() {
                let (tx, rx) = futures::sync::mpsc::channel(0);

                txs.insert(name, tx.sink_map_err(sink_map_err));

                let sink_task = sink
                    .map_err(|e| error!("error creating sender: {:?}", e))
                    .and_then(|sink| rx.forward(sink).map(|_| ()));

                tokio::spawn(sink_task);
            }

            let mut transform_rxs = vec![];

            for (name, transform) in self.transforms.into_iter() {
                let (tx, rx) = futures::sync::mpsc::channel(0);

                txs.insert(name.clone(), tx.sink_map_err(sink_map_err));

                let rx = rx.filter_map(move |r| transform.transform(r));
                transform_rxs.push((name, rx));
            }

            let connections = &self.connections;
            let outs = |name| -> Box<dyn Sink<SinkItem = Record, SinkError = ()> + Send> {
                let out_names = connections.get(&name).unwrap();
                let mut outs = out_names
                    .iter()
                    .map(|ref out_name| Box::new(txs[*out_name].clone()));
                let first_out = outs.next().unwrap();
                let out: Box<dyn Sink<SinkItem = Record, SinkError = ()> + Send> =
                    outs.fold(first_out, |a, b| Box::new(a.fanout(b)));

                out
            };

            for (name, rx) in transform_rxs.into_iter() {
                let transform_task = rx.forward(outs(name)).map(|_| ());
                tokio::spawn(transform_task);
            }

            let servers = self.sources.iter().map(|(name, source)| {
                let (tx, rx) = futures::sync::mpsc::channel(1000);

                let pump_task = rx.forward(outs(name.clone())).map(|_| ());
                tokio::spawn(pump_task);

                source(tx)
            });

            for server in servers {
                let server = server.select(tripwire.clone()).map(|_| ()).map_err(|_| ());
                tokio::spawn(server);
            }

            future::ok(())
        });

        (lazy, trigger)
    }
}
