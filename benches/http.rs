use criterion::{criterion_group, Benchmark, Criterion, Throughput};
use futures::Future;
use hyper::service::service_fn_ok;
use hyper::{Body, Response, Server};
use std::net::SocketAddr;
use vector::test_util::{next_addr, random_lines, send_lines, wait_for_tcp};
use vector::{
    sinks, sources,
    topology::{config, Topology},
};

fn benchmark_http_no_compression(c: &mut Criterion) {
    let num_lines: usize = 100_000;
    let line_size: usize = 100;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let bench = Benchmark::new("http_no_compression", move |b| {
        b.iter_with_setup(
            || {
                let mut config = config::Config::empty();
                config.add_source("in", sources::tcp::TcpConfig::new(in_addr));
                config.add_sink(
                    "out",
                    &["in"],
                    sinks::http::HttpSinkConfig {
                        uri: out_addr.to_string(),
                        compression: Some(sinks::util::Compression::None),
                        ..Default::default()
                    },
                );
                let (mut topology, _warnings) = Topology::build(config).unwrap();

                let mut rt = tokio::runtime::Runtime::new().unwrap();

                rt.spawn(serve(out_addr));

                topology.start(&mut rt);
                wait_for_tcp(in_addr);

                (rt, topology)
            },
            |(mut rt, mut topology)| {
                let send = send_lines(in_addr, random_lines(line_size).take(num_lines));
                rt.block_on(send).unwrap();

                rt.block_on(topology.stop()).unwrap();

                rt.shutdown_now().wait().unwrap();
            },
        )
    })
    .sample_size(10)
    .noise_threshold(0.05)
    .throughput(Throughput::Bytes((num_lines * line_size) as u32));

    c.bench("http", bench);
}

fn benchmark_http_gzip(c: &mut Criterion) {
    let num_lines: usize = 100_000;
    let line_size: usize = 100;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let bench = Benchmark::new("http_gzip", move |b| {
        b.iter_with_setup(
            || {
                let mut config = config::Config::empty();
                config.add_source("in", sources::tcp::TcpConfig::new(in_addr));
                config.add_sink(
                    "out",
                    &["in"],
                    sinks::http::HttpSinkConfig {
                        uri: out_addr.to_string(),
                        ..Default::default()
                    },
                );
                let (mut topology, _warnings) = Topology::build(config).unwrap();

                let mut rt = tokio::runtime::Runtime::new().unwrap();

                rt.spawn(serve(out_addr));

                topology.start(&mut rt);
                wait_for_tcp(in_addr);

                (rt, topology)
            },
            |(mut rt, mut topology)| {
                let send = send_lines(in_addr, random_lines(line_size).take(num_lines));
                rt.block_on(send).unwrap();

                rt.block_on(topology.stop()).unwrap();

                rt.shutdown_now().wait().unwrap();
            },
        )
    })
    .sample_size(10)
    .noise_threshold(0.05)
    .throughput(Throughput::Bytes((num_lines * line_size) as u32));

    c.bench("http", bench);
}

fn serve(addr: SocketAddr) -> impl Future<Item = (), Error = ()> {
    let make_service = || service_fn_ok(|_req| Response::new(Body::empty()));

    Server::bind(&addr)
        .serve(make_service)
        .map_err(|e| panic!(e))
}

criterion_group!(http, benchmark_http_no_compression, benchmark_http_gzip);
