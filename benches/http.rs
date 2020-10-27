use criterion::{criterion_group, Benchmark, Criterion, Throughput};
use futures::{compat::Future01CompatExt, TryFutureExt};
use hyper::{
    service::{make_service_fn, service_fn},
    Body, Response, Server,
};
use std::net::SocketAddr;
use tokio::runtime::Runtime;
use vector::{
    config, sinks, sources,
    test_util::{next_addr, random_lines, runtime, send_lines, start_topology, wait_for_tcp},
    Error,
};

fn benchmark_http_no_compression(c: &mut Criterion) {
    let num_lines: usize = 100_000;
    let line_size: usize = 100;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let _srv = serve(out_addr);

    let bench = Benchmark::new("http_no_compression", move |b| {
        b.iter_with_setup(
            || {
                let mut config = config::Config::builder();
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

                let mut rt = runtime();
                let topology = rt.block_on(async move {
                    let (topology, _crash) = start_topology(config.build().unwrap(), false).await;
                    wait_for_tcp(in_addr).await;
                    topology
                });
                (rt, topology)
            },
            |(mut rt, topology)| {
                rt.block_on(async move {
                    let lines = random_lines(line_size).take(num_lines);
                    send_lines(in_addr, lines).await.unwrap();
                    topology.stop().compat().await.unwrap();
                });
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

    let _srv = serve(out_addr);

    let bench = Benchmark::new("http_gzip", move |b| {
        b.iter_with_setup(
            || {
                let mut config = config::Config::builder();
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

                let mut rt = runtime();
                let topology = rt.block_on(async move {
                    let (topology, _crash) = start_topology(config.build().unwrap(), false).await;
                    wait_for_tcp(in_addr).await;
                    topology
                });
                (rt, topology)
            },
            |(mut rt, topology)| {
                rt.block_on(async move {
                    let lines = random_lines(line_size).take(num_lines);
                    send_lines(in_addr, lines).await.unwrap();
                    topology.stop().compat().await.unwrap();
                });
            },
        )
    })
    .sample_size(10)
    .noise_threshold(0.05)
    .throughput(Throughput::Bytes((num_lines * line_size) as u64));

    c.bench("http", bench);
}

fn serve(addr: SocketAddr) -> Runtime {
    let rt = runtime();
    rt.spawn(async move {
        let make_service = make_service_fn(|_| async {
            Ok::<_, Error>(service_fn(|_req| async {
                Ok::<_, Error>(Response::new(Body::empty()))
            }))
        });

        Server::bind(&addr)
            .serve(make_service)
            .map_err(|e| panic!(e))
            .await
    });
    rt
}

criterion_group!(http, benchmark_http_no_compression, benchmark_http_gzip);
