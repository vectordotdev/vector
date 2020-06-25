use criterion::{criterion_group, Benchmark, Criterion, Throughput};
use futures::TryFutureExt;
use futures01::Future;
use hyper::{
    service::{make_service_fn, service_fn},
    Body, Response, Server,
};
use std::net::SocketAddr;
use vector::test_util::{next_addr, random_lines, send_lines, wait_for_tcp};
use vector::{
    runtime, sinks, sources,
    topology::{self, config},
    Error,
};

fn benchmark_http_no_compression(c: &mut Criterion) {
    let num_lines: usize = 100_000;
    let line_size: usize = 100;

    let in_addr = next_addr();
    let out_addr = next_addr();

    serve(out_addr);

    let bench = Benchmark::new("http_no_compression", move |b| {
        b.iter_with_setup(
            || {
                let mut config = config::Config::empty();
                config.add_source(
                    "in",
                    sources::socket::SocketConfig::make_tcp_config(in_addr),
                );
                config.add_sink(
                    "out",
                    &["in"],
                    sinks::http::HttpSinkConfig {
                        uri: out_addr.to_string().parse::<http::Uri>().unwrap().into(),
                        compression: sinks::util::Compression::None,
                        method: Default::default(),
                        healthcheck_uri: Default::default(),
                        auth: Default::default(),
                        headers: Default::default(),
                        batch: Default::default(),
                        encoding: sinks::http::Encoding::Text.into(),
                        request: Default::default(),
                        tls: Default::default(),
                    },
                );

                let mut rt = runtime::Runtime::new().unwrap();

                let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
                wait_for_tcp(in_addr);

                (rt, topology)
            },
            |(mut rt, topology)| {
                let send = send_lines(in_addr, random_lines(line_size).take(num_lines));
                rt.block_on(send).unwrap();

                rt.block_on(topology.stop()).unwrap();

                rt.shutdown_now().wait().unwrap();
            },
        )
    })
    .sample_size(10)
    .noise_threshold(0.05)
    .throughput(Throughput::Bytes((num_lines * line_size) as u64));

    c.bench("http", bench);
}

fn benchmark_http_gzip(c: &mut Criterion) {
    let num_lines: usize = 100_000;
    let line_size: usize = 100;

    let in_addr = next_addr();
    let out_addr = next_addr();

    serve(out_addr);

    let bench = Benchmark::new("http_gzip", move |b| {
        b.iter_with_setup(
            || {
                let mut config = config::Config::empty();
                config.add_source(
                    "in",
                    sources::socket::SocketConfig::make_tcp_config(in_addr),
                );
                config.add_sink(
                    "out",
                    &["in"],
                    sinks::http::HttpSinkConfig {
                        uri: out_addr.to_string().parse::<http::Uri>().unwrap().into(),
                        compression: Default::default(),
                        method: Default::default(),
                        healthcheck_uri: Default::default(),
                        auth: Default::default(),
                        headers: Default::default(),
                        batch: Default::default(),
                        encoding: sinks::http::Encoding::Text.into(),
                        request: Default::default(),
                        tls: Default::default(),
                    },
                );

                let mut rt = runtime::Runtime::new().unwrap();

                let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
                wait_for_tcp(in_addr);

                (rt, topology)
            },
            |(mut rt, topology)| {
                let send = send_lines(in_addr, random_lines(line_size).take(num_lines));
                rt.block_on(send).unwrap();

                rt.block_on(topology.stop()).unwrap();

                rt.shutdown_now().wait().unwrap();
            },
        )
    })
    .sample_size(10)
    .noise_threshold(0.05)
    .throughput(Throughput::Bytes((num_lines * line_size) as u64));

    c.bench("http", bench);
}

fn serve(addr: SocketAddr) {
    std::thread::spawn(move || {
        let make_service = make_service_fn(|_| async {
            Ok::<_, Error>(service_fn(|_req| async {
                Ok::<_, Error>(Response::new(Body::empty()))
            }))
        });

        let fut = Server::bind(&addr)
            .serve(make_service)
            .compat()
            .map_err(|e| panic!(e));

        tokio01::runtime::current_thread::run(fut);
    });
}

criterion_group!(http, benchmark_http_no_compression, benchmark_http_gzip);
