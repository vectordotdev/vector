use futures::prelude::*;
use futures::{future, Future};
use log::error;
use sinks;
use sources;
use std::collections::HashMap;
use stream_cancel::{Trigger, Tripwire};
use Record;

pub struct TopologyBuilder {
    sources: HashMap<String, sources::Source>,
    sinks: HashMap<String, sinks::RouterSinkFuture>,
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

    // TODO: transforms

    pub fn connect(&mut self, in_name: &str, out_name: &str) {
        self.connections
            .entry(in_name.to_owned())
            .or_insert_with(Vec::new)
            .push(out_name.to_owned());
    }

    // TODO: warn/error on unconnected elements
    pub fn build(mut self) -> (impl Future<Item = (), Error = ()>, Trigger) {
        let mut to_join: Vec<Box<dyn Future<Item = (), Error = ()> + Send>> = vec![];

        let mut txs = HashMap::new();

        for (name, sink) in self.sinks.into_iter() {
            let (tx, rx) = futures::sync::mpsc::channel(0);

            txs.insert(name, tx.sink_map_err(|e| error!("sender error: {:?}", e)));

            let sink_fut = sink
                .map(|sink| sink.sink_map_err(|e| error!("sender error: {:?}", e)))
                .map_err(|e| error!("error creating sender: {:?}", e))
                .and_then(|sink| rx.forward(sink).map(|_| ()));
            to_join.push(Box::new(sink_fut));
        }

        for (in_, outs) in self.connections {
            let mut outs = outs
                .into_iter()
                .map(|out_name| Box::new(txs[&out_name].clone()));
            let first_out = outs.next().unwrap();
            let out: Box<dyn Sink<SinkItem = Record, SinkError = ()> + Send> =
                outs.fold(first_out, |a, b| Box::new(a.fanout(b)));

            let in_ = self.sources.remove(&in_).unwrap();

            let fut = in_.forward(out).map(|_| ());
            to_join.push(Box::new(fut));
        }

        (future::join_all(to_join).map(|_| ()), self.trigger)
    }
}
