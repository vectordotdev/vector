extern crate futures;
extern crate log;
extern crate prometheus;
extern crate regex;
extern crate router;
extern crate stream_cancel;
extern crate tokio;

use futures::{Future, Sink, Stream};
use log::{error, info};
use prometheus::{opts, register_counter, Encoder, TextEncoder, __register_counter};
use router::{sinks, sources, topology};
use std::net::SocketAddr;
use stream_cancel::{Trigger, Tripwire};
use tokio::fs::File;

fn main() {
    router::setup_logger();

    let in_addr: SocketAddr = "127.0.0.1:1235".parse().unwrap();
    let (harness_trigger, tripwire) = Tripwire::new();

    info!("starting runtime");
    let mut rt = tokio::runtime::Runtime::new().unwrap();

    // TODO: actually switch between configurations in a reasonable way (separate binaries?)
    if false {
        // ES Writer topology
        let (server, _trigger) = es_writer(in_addr);
        rt.spawn(server);
    } else {
        // Comcast topology + input and harness
        let out_addr: SocketAddr = "127.0.0.1:9999".parse().unwrap();
        let (server, server_trigger) = comcast(in_addr, out_addr);

        // build up a thing that will pipe some sample data at our server
        let input = File::open("sample.log")
            .map_err(|e| error!("error opening file: {:?}", e))
            .map(sources::reader_source)
            .flatten_stream();
        let sender = sinks::splunk::raw_tcp(in_addr)
            .map(|sink| sink.sink_map_err(|e| error!("sender error: {:?}", e)))
            .map_err(|e| error!("error creating sender: {:?}", e));
        let counter = register_counter!("sender_lines", "Lines sent from harness").unwrap();
        let sender_task = sender
            .and_then(|sink| input.inspect(move |_| counter.inc()).forward(sink))
            .map(|_| {
                info!("done sending test input!");
                drop(server_trigger);
                drop(harness_trigger);
            });

        let receiver = {
            let counter =
                register_counter!("receiver_lines", "Lines received at forwarding destination")
                    .unwrap();
            let (tx, rx) = futures::sync::mpsc::channel(10);

            rt.spawn(rx.for_each(move |_| {
                counter.inc();
                Ok(())
            }));

            sources::splunk::raw_tcp(out_addr, tx)
        };

        info!("starting receiver");
        rt.spawn(
            receiver
                .select(tripwire.clone())
                .map(|_| ())
                .map_err(|_| ()),
        );

        info!("starting server");
        rt.spawn(server);
        // wait for the server to come up before trying to send to it
        while let Err(_) = std::net::TcpStream::connect(in_addr) {}

        info!("starting sender");
        rt.block_on(sender_task).unwrap();
        info!("sender finished!");
    }

    rt.shutdown_on_idle().wait().unwrap();

    let mut buf = Vec::new();
    let encoder = TextEncoder::new();
    let metrics_families = prometheus::gather();
    encoder.encode(&metrics_families, &mut buf).unwrap();
    info!("prom output:\n{}", String::from_utf8(buf).unwrap());
}

fn es_writer(in_addr: SocketAddr) -> (impl Future<Item = (), Error = ()>, Trigger) {
    let mut topology = topology::config::Config::empty();
    topology.add_source("in", topology::config::Source::Splunk { address: in_addr });
    topology.add_sink("out", &["in"], topology::config::Sink::Elasticsearch);
    topology::build(topology)
}

// the server topology we'd actally use for the comcast use case
fn comcast(
    in_addr: SocketAddr,
    out_addr: SocketAddr,
) -> (impl Future<Item = (), Error = ()>, Trigger) {
    let mut topology = topology::config::Config::empty();
    topology.add_source("in", topology::config::Source::Splunk { address: in_addr });
    topology.add_transform(
        "sampler",
        &["in"],
        topology::config::Transform::Sampler {
            rate: 10,
            pass_list: vec![],
        },
    );
    topology.add_sink(
        "out",
        &["sampler"],
        topology::config::Sink::Splunk { address: out_addr },
    );
    topology::build(topology)
}
