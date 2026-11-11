use std::{convert::Infallible, net::SocketAddr};

use bytes::Bytes;
use criterion::{BatchSize, BenchmarkId, Criterion, SamplingMode, Throughput, criterion_group};
use http::Response;
use http_body_util::Empty;
use hyper::service::service_fn;
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto,
};
use tokio::{net::TcpListener, runtime::Runtime};
use vector::{
    config,
    sinks::{
        self,
        util::{BatchConfig, Compression},
    },
    sources,
    template::UriTemplate,
    test_util::{addr::next_addr, random_lines, runtime, send_lines, start_topology, wait_for_tcp},
};
use vector_lib::codecs::{TextSerializerConfig, encoding::FramingConfig};

// https://github.com/vectordotdev/vector/issues/23659
#[allow(
    clippy::default_trait_access,
    reason = "Keep inferred defaults where concrete type names need a separate import or API cleanup."
)]
fn benchmark_http(c: &mut Criterion) {
    let num_lines: usize = 1_000;
    let line_size: usize = 100;

    let (_guard_0, in_addr) = next_addr();
    let (_guard_1, out_addr) = next_addr();

    let _srv = serve(out_addr);

    let mut group = c.benchmark_group("http");
    group.throughput(Throughput::Bytes((num_lines * line_size) as u64));
    group.sampling_mode(SamplingMode::Flat);

    for compression in &[Compression::None, Compression::gzip_default()] {
        group.bench_with_input(
            BenchmarkId::new("compression", compression),
            compression,
            |b, compression| {
                b.iter_batched(
                    || {
                        let mut config = config::Config::builder();
                        config.add_source(
                            "in",
                            sources::socket::SocketConfig::make_basic_tcp_config(in_addr),
                        );
                        let mut batch = BatchConfig::default();
                        batch.max_bytes = Some(num_lines * line_size);

                        config.add_sink(
                            "out",
                            &["in"],
                            sinks::http::config::HttpSinkConfig {
                                uri: UriTemplate::try_from(format!("http://{out_addr}")).unwrap(),
                                compression: *compression,
                                method: Default::default(),
                                auth: Option::default(),
                                payload_prefix: String::default(),
                                payload_suffix: String::default(),
                                batch,
                                encoding: (None::<FramingConfig>, TextSerializerConfig::default())
                                    .into(),
                                request: Default::default(),
                                tls: Option::default(),
                                acknowledgements: Default::default(),
                                retry_strategy: Default::default(),
                                confinement: Default::default(),
                            },
                        );

                        let rt = runtime();
                        let topology = rt.block_on(async move {
                            let (topology, _crash) =
                                start_topology(config.build().unwrap(), false).await;
                            wait_for_tcp(in_addr).await;
                            topology
                        });
                        (rt, topology)
                    },
                    |(rt, topology)| {
                        rt.block_on(async move {
                            let lines = random_lines(line_size).take(num_lines);
                            send_lines(in_addr, lines).await.unwrap();
                            topology.stop().await;
                        });
                    },
                    BatchSize::PerIteration,
                );
            },
        );
    }

    group.finish();
}

fn serve(addr: SocketAddr) -> Runtime {
    let rt = runtime();
    let listener = rt.block_on(TcpListener::bind(addr)).unwrap();
    rt.spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                // Connection errors do not stop the server.
                _ = auto::Builder::new(TokioExecutor::new())
                    .serve_connection(
                        TokioIo::new(stream),
                        service_fn(|_req| async {
                            Ok::<_, Infallible>(Response::new(Empty::<Bytes>::new()))
                        }),
                    )
                    .await;
            });
        }
    });
    rt
}

criterion_group!(benches, benchmark_http);
