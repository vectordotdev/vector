use criterion::{criterion_group, Benchmark, Criterion, Throughput};

use futures::{future, Future, Stream};
use router::test_util::{next_addr, send_lines, shutdown_on_idle, wait_for_tcp};
use router::topology::{config, Topology};
use router::{buffers::BufferConfig, sinks, sources};
use std::net::SocketAddr;
use tempfile::tempdir;
use tokio::codec::{FramedRead, LinesCodec};
use tokio::net::TcpListener;

fn benchmark_buffers(c: &mut Criterion) {
    let num_lines: usize = 100_000;
    let line_size: usize = 100;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let data_dir = tempdir().unwrap();
    let data_dir = data_dir.path().to_path_buf();
    let data_dir2 = data_dir.clone();

    c.bench(
        "buffers",
        Benchmark::new("in-memory", move |b| {
            b.iter_with_setup(
                || {
                    let mut config = config::Config::empty();
                    config.add_source(
                        "in",
                        sources::tcp::TcpConfig {
                            address: in_addr,
                            max_length: 102400,
                        },
                    );
                    config.add_sink(
                        "out",
                        &["in"],
                        sinks::tcp::TcpSinkConfig { address: out_addr },
                    );
                    config.sinks["out"].buffer = BufferConfig::Memory { num_items: 100 };
                    let (mut topology, _warnings) = Topology::build(config).unwrap();

                    let mut rt = tokio::runtime::Runtime::new().unwrap();

                    let output_lines = count_lines(&out_addr, &rt.executor());

                    topology.start(&mut rt);
                    wait_for_tcp(in_addr);

                    (rt, topology, output_lines)
                },
                |(mut rt, mut topology, output_lines)| {
                    let send = send_lines(in_addr, random_lines(line_size).take(num_lines));
                    rt.block_on(send).unwrap();

                    topology.stop();

                    shutdown_on_idle(rt);
                    assert_eq!(num_lines, output_lines.wait().unwrap());
                },
            );
        })
        .with_function("on-disk", move |b| {
            b.iter_with_setup(
                || {
                    let mut config = config::Config::empty();
                    config.add_source(
                        "in",
                        sources::tcp::TcpConfig {
                            address: in_addr,
                            max_length: 102400,
                        },
                    );
                    config.add_sink(
                        "out",
                        &["in"],
                        sinks::tcp::TcpSinkConfig { address: out_addr },
                    );
                    config.sinks["out"].buffer = BufferConfig::Disk {
                        max_size: 1_000_000,
                    };
                    config.data_dir = Some(data_dir.clone());
                    let (mut topology, _warnings) = Topology::build(config).unwrap();

                    let mut rt = tokio::runtime::Runtime::new().unwrap();

                    let output_lines = count_lines(&out_addr, &rt.executor());

                    topology.start(&mut rt);
                    wait_for_tcp(in_addr);

                    (rt, topology, output_lines)
                },
                |(mut rt, mut topology, output_lines)| {
                    let send = send_lines(in_addr, random_lines(line_size).take(num_lines));
                    rt.block_on(send).unwrap();

                    topology.stop();

                    shutdown_on_idle(rt);
                    assert_eq!(num_lines, output_lines.wait().unwrap());
                },
            );
        })
        .with_function("on-disk (low limit)", move |b| {
            b.iter_with_setup(
                || {
                    let mut config = config::Config::empty();
                    config.add_source(
                        "in",
                        sources::tcp::TcpConfig {
                            address: in_addr,
                            max_length: 102400,
                        },
                    );
                    config.add_sink(
                        "out",
                        &["in"],
                        sinks::tcp::TcpSinkConfig { address: out_addr },
                    );
                    config.sinks["out"].buffer = BufferConfig::Disk { max_size: 10_000 };
                    config.data_dir = Some(data_dir2.clone());
                    let (mut topology, _warnings) = Topology::build(config).unwrap();

                    let mut rt = tokio::runtime::Runtime::new().unwrap();

                    let output_lines = count_lines(&out_addr, &rt.executor());

                    topology.start(&mut rt);
                    wait_for_tcp(in_addr);

                    (rt, topology, output_lines)
                },
                |(mut rt, mut topology, output_lines)| {
                    let send = send_lines(in_addr, random_lines(line_size).take(num_lines));
                    rt.block_on(send).unwrap();

                    topology.stop();

                    shutdown_on_idle(rt);
                    assert_eq!(num_lines, output_lines.wait().unwrap());
                },
            );
        })
        .sample_size(4)
        .noise_threshold(0.05)
        .throughput(Throughput::Bytes((num_lines * line_size) as u32)),
    );
}

criterion_group!(buffers, benchmark_buffers);

fn random_lines(size: usize) -> impl Iterator<Item = String> {
    use rand::distributions::Alphanumeric;
    use rand::{rngs::SmallRng, thread_rng, Rng, SeedableRng};

    let mut rng = SmallRng::from_rng(thread_rng()).unwrap();

    std::iter::repeat(()).map(move |_| {
        rng.sample_iter(&Alphanumeric)
            .take(size)
            .collect::<String>()
    })
}

fn count_lines(
    addr: &SocketAddr,
    executor: &tokio::runtime::TaskExecutor,
) -> impl Future<Item = usize, Error = ()> {
    let listener = TcpListener::bind(addr).unwrap();

    let lines = listener
        .incoming()
        .take(1)
        .map(|socket| FramedRead::new(socket, LinesCodec::new()))
        .flatten()
        .map_err(|e| panic!("{:?}", e))
        .fold(0, |n, _| future::ok(n + 1));

    futures::sync::oneshot::spawn(lines, executor)
}
