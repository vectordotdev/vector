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
use router::{
    sinks::{self, SinkFactory},
    sources::{self, SourceFactory},
    topology::TopologyBuilder,
    transforms,
};
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

        rt.spawn(es_writer(in_addr, tripwire));
    } else {
        // Comcast topology + input and harness
        let out_addr: SocketAddr = "127.0.0.1:9999".parse().unwrap();
        let (server, server_trigger) = comcast(in_addr, out_addr);

        // build up a thing that will pipe some sample data at our server
        let input = File::open("sample.log")
            .map_err(|e| error!("error opening file: {:?}", e))
            .map(sources::reader_source)
            .flatten_stream();
        let sender = sinks::splunk::SplunkSink::build(in_addr)
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

        // build up a thing that accept the data our server forwards upstream
        let counter =
            register_counter!("receiver_lines", "Lines received at forwarding destination")
                .unwrap();
        let receiver =
            sources::splunk::SplunkSource::build(out_addr, tripwire).fold((), move |(), _line| {
                counter.inc();
                Ok(())
            });

        info!("starting receiver");
        rt.spawn(receiver);

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

fn es_writer(in_addr: SocketAddr, exit: Tripwire) -> impl Future<Item = (), Error = ()> {
    let counter = register_counter!("input_lines", "Lines ingested").unwrap();
    let splunk_in =
        sources::splunk::SplunkSource::build(in_addr, exit).inspect(move |_| counter.inc());

    let sink = sinks::elasticsearch::ElasticseachSink::new()
        .sink_map_err(|e| error!("es sink error: {:?}", e));
    sink.send_all(splunk_in).map(|_| info!("done!"))
}

// the server topology we'd actally use for the comcast use case
fn comcast(
    in_addr: SocketAddr,
    out_addr: SocketAddr,
) -> (impl Future<Item = (), Error = ()>, Trigger) {
    let mut topology = TopologyBuilder::new();

    topology.add_source::<sources::splunk::SplunkSource>(in_addr, "in");
    topology.add_sink::<sinks::splunk::SplunkSink>(out_addr, "out");
    let sampler_config = transforms::SamplerConfig {
        rate: 10,
        pass_list: vec!["(very )?important".to_string()],
    };
    topology.add_transform::<transforms::Sampler>(sampler_config, "sampler");

    topology.connect("in", "sampler");
    topology.connect("sampler", "out");

    topology.build()
}
