extern crate router;

#[macro_use]
extern crate log;
extern crate futures;
extern crate regex;
extern crate tokio;

use futures::{Future, Sink, Stream};
use regex::bytes::RegexSet;
use router::{sinks, sources, transforms};
use std::net::SocketAddr;
use tokio::fs::File;

fn main() {
    router::setup_logger();
    let in_addr: SocketAddr = "127.0.0.1:1235".parse().unwrap();
    let out_addr: SocketAddr = "127.0.0.1:9999".parse().unwrap();

    // build up a thing that will pipe some sample data at our server
    let input = File::open("sample.log")
        .map_err(|e| error!("error opening file: {:?}", e))
        .map(sources::reader_source)
        .flatten_stream();
    let sender = sinks::splunk::raw_tcp("sender_out", in_addr)
        .map(|sink| sink.sink_map_err(|e| error!("sender error: {:?}", e)))
        .map_err(|e| error!("error creating sender: {:?}", e));
    let sender_task = sender
        .and_then(|sink| input.forward(sink))
        .map(|_| info!("done sending test input!"));

    // build up a thing that accept the data our server forwards upstream
    let receiver = sources::splunk::raw_tcp(out_addr)
        .fold((0usize, 0usize), |(count, bytes), line| {
            let count = count + 1;
            let bytes = bytes + line.len();
            if count % 10_000 == 0 {
                info!("{} lines ({} bytes) so far", count, bytes);
            }
            Ok((count, bytes))
        }).map(|(count, bytes)| info!("{} total lines ({} total bytes)", count, bytes));

    info!("starting runtime");
    let mut rt = tokio::runtime::Runtime::new().unwrap();

    info!("starting receiver");
    rt.spawn(receiver);

    info!("starting server");
    rt.spawn(comcast(in_addr, out_addr));

    info!("starting sender");
    rt.block_on_all(sender_task).unwrap();

    info!("sender finished!");
}

// the server topology we'd actally use for the comcast use case
fn comcast(in_addr: SocketAddr, out_addr: SocketAddr) -> impl Future<Item = (), Error = ()> {
    let splunk_in = sources::splunk::raw_tcp(in_addr);

    let exceptions = RegexSet::new(&["(very )?important"]).unwrap();
    let mut sampler = transforms::Sampler::new(100, exceptions);
    let sampled = splunk_in.filter(move |record| sampler.filter(record.as_bytes()));

    let splunk_out = sinks::splunk::raw_tcp("server_out", out_addr)
        .map(|sink| sink.sink_map_err(|e| error!("tcp sink error: {:?}", e)))
        .map_err(|e| error!("error creating tcp sink: {:?}", e));

    splunk_out
        .and_then(|sink| sampled.forward(sink))
        .map(|_| info!("done!"))
}
