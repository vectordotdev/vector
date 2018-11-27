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
use regex::bytes::RegexSet;
use router::{sinks, sources, transforms};
use std::net::SocketAddr;
use stream_cancel::Tripwire;
use tokio::fs::File;

fn main() {
    router::setup_logger();

    let in_addr: SocketAddr = "127.0.0.1:1235".parse().unwrap();
    let (trigger, tripwire) = Tripwire::new();

    info!("starting runtime");
    let mut rt = tokio::runtime::Runtime::new().unwrap();

    // TODO: actually switch between configurations in a reasonable way (separate binaries?)
    if false {
        // ES Writer topology
        rt.spawn(es_writer(in_addr, tripwire));
    } else {
        // Comcast topology + input and harness

        // build up a thing that will pipe some sample data at our server
        let input = File::open("sample.log")
            .map_err(|e| error!("error opening file: {:?}", e))
            .map(sources::reader_source)
            .flatten_stream();
        let sender = sinks::splunk::raw_tcp(in_addr)
            .map(|sink| sink.sink_map_err(|e| error!("sender error: {:?}", e)))
            .map_err(|e| error!("error creating sender: {:?}", e));
        let sender_task = sender.and_then(|sink| input.forward(sink)).map(|_| {
            info!("done sending test input!");
            drop(trigger);
        });

        // build up a thing that accept the data our server forwards upstream
        let out_addr: SocketAddr = "127.0.0.1:9999".parse().unwrap();
        let counter =
            register_counter!("receiver_lines", "Lines received at forwarding destination")
                .unwrap();
        let receiver =
            sources::splunk::raw_tcp(out_addr, tripwire.clone()).fold((), move |(), _line| {
                counter.inc();
                Ok(())
            });

        info!("starting receiver");
        rt.spawn(receiver);

        info!("starting server");
        rt.spawn(comcast(in_addr, out_addr, tripwire));

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
    let splunk_in = sources::splunk::raw_tcp(in_addr, exit).inspect(move |_| counter.inc());

    let sink = sinks::elasticsearch::ElasticseachSink::new()
        .sink_map_err(|e| error!("es sink error: {:?}", e));
    sink.send_all(splunk_in).map(|_| info!("done!"))
}

// the server topology we'd actally use for the comcast use case
fn comcast(
    in_addr: SocketAddr,
    out_addr: SocketAddr,
    exit: Tripwire,
) -> impl Future<Item = (), Error = ()> {
    let counter = register_counter!("input_lines", "Lines ingested").unwrap();

    let splunk_in = sources::splunk::raw_tcp(in_addr, exit).inspect(move |_| counter.inc());

    let counter = register_counter!("output_lines", "Lines forwarded upstream").unwrap();

    let exceptions = RegexSet::new(&["(very )?important"]).unwrap();
    let mut sampler = transforms::Sampler::new(10, exceptions);
    let sampled = splunk_in
        .filter(move |record| sampler.filter(record.as_bytes()))
        .inspect(move |_| counter.inc());

    let splunk_out = sinks::splunk::raw_tcp(out_addr)
        .map(|sink| sink.sink_map_err(|e| error!("tcp sink error: {:?}", e)))
        .map_err(|e| error!("error creating tcp sink: {:?}", e));

    splunk_out
        .and_then(|sink| sampled.forward(sink))
        .map(|_| info!("done!"))
}
