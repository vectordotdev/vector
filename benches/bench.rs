use criterion::{criterion_group, criterion_main, Benchmark, Criterion, Throughput};

use approx::assert_relative_eq;
use futures::{future, Future, Stream};
use router::test_util::{next_addr, send_lines};
use router::topology::{self, config};
use router::{sinks, sources, transforms};
use std::net::SocketAddr;
use tokio::codec::{FramedRead, LinesCodec};
use tokio::net::TcpListener;

fn benchmark_simple_pipe(c: &mut Criterion) {
    let num_lines: usize = 100_000;
    let line_size: usize = 100;

    let in_addr = next_addr();
    let out_addr = next_addr();

    c.bench(
        "pipe",
        Benchmark::new("pipe", move |b| {
            b.iter_with_setup(
                || {
                    let mut topology = config::Config::empty();
                    topology.add_source(
                        "in",
                        sources::tcp::TcpConfig {
                            address: in_addr,
                            max_length: 102400,
                        },
                    );
                    topology.add_sink(
                        "out",
                        &["in"],
                        sinks::splunk::TcpSinkConfig { address: out_addr },
                    );
                    let (server, trigger, _healthchecks, _warnings) =
                        topology::build(topology).unwrap();

                    let mut rt = tokio::runtime::Runtime::new().unwrap();

                    let output_lines = count_lines(&out_addr, &rt.executor());

                    rt.spawn(server);
                    while let Err(_) = std::net::TcpStream::connect(in_addr) {}

                    (rt, trigger, output_lines)
                },
                |(mut rt, trigger, output_lines)| {
                    let send = send_lines(in_addr, random_lines(line_size).take(num_lines));
                    rt.block_on(send).unwrap();

                    drop(trigger);

                    rt.shutdown_on_idle().wait().unwrap();
                    assert_eq!(num_lines, output_lines.wait().unwrap());
                },
            );
        })
        .sample_size(4)
        .noise_threshold(0.05)
        .throughput(Throughput::Bytes((num_lines * line_size) as u32)),
    );
}

fn benchmark_simple_pipe_with_tiny_lines(c: &mut Criterion) {
    let num_lines: usize = 100_000;
    let line_size: usize = 1;

    let in_addr = next_addr();
    let out_addr = next_addr();

    c.bench(
        "pipe_with_tiny_lines",
        Benchmark::new("pipe_with_tiny_lines", move |b| {
            b.iter_with_setup(
                || {
                    let mut topology = config::Config::empty();
                    topology.add_source(
                        "in",
                        sources::tcp::TcpConfig {
                            address: in_addr,
                            max_length: 102400,
                        },
                    );
                    topology.add_sink(
                        "out",
                        &["in"],
                        sinks::splunk::TcpSinkConfig { address: out_addr },
                    );
                    let (server, trigger, _healthchecks, _warnings) =
                        topology::build(topology).unwrap();

                    let mut rt = tokio::runtime::Runtime::new().unwrap();

                    let output_lines = count_lines(&out_addr, &rt.executor());

                    rt.spawn(server);
                    while let Err(_) = std::net::TcpStream::connect(in_addr) {}

                    (rt, trigger, output_lines)
                },
                |(mut rt, trigger, output_lines)| {
                    let send = send_lines(in_addr, random_lines(line_size).take(num_lines));
                    rt.block_on(send).unwrap();

                    drop(trigger);

                    rt.shutdown_on_idle().wait().unwrap();
                    assert_eq!(num_lines, output_lines.wait().unwrap());
                },
            );
        })
        .sample_size(4)
        .noise_threshold(0.05)
        .throughput(Throughput::Bytes((num_lines * line_size) as u32)),
    );
}

fn benchmark_simple_pipe_with_huge_lines(c: &mut Criterion) {
    let num_lines: usize = 2_000;
    let line_size: usize = 100_000;

    let in_addr = next_addr();
    let out_addr = next_addr();

    c.bench(
        "pipe_with_huge_lines",
        Benchmark::new("pipe_with_huge_lines", move |b| {
            b.iter_with_setup(
                || {
                    let mut topology = config::Config::empty();
                    topology.add_source(
                        "in",
                        sources::tcp::TcpConfig {
                            address: in_addr,
                            max_length: 102400,
                        },
                    );
                    topology.add_sink(
                        "out",
                        &["in"],
                        sinks::splunk::TcpSinkConfig { address: out_addr },
                    );
                    let (server, trigger, _healthchecks, _warnings) =
                        topology::build(topology).unwrap();

                    let mut rt = tokio::runtime::Runtime::new().unwrap();

                    let output_lines = count_lines(&out_addr, &rt.executor());

                    rt.spawn(server);
                    while let Err(_) = std::net::TcpStream::connect(in_addr) {}

                    (rt, trigger, output_lines)
                },
                |(mut rt, trigger, output_lines)| {
                    let send = send_lines(in_addr, random_lines(line_size).take(num_lines));
                    rt.block_on(send).unwrap();

                    drop(trigger);

                    rt.shutdown_on_idle().wait().unwrap();
                    assert_eq!(num_lines, output_lines.wait().unwrap());
                },
            );
        })
        .sample_size(4)
        .noise_threshold(0.05)
        .throughput(Throughput::Bytes((num_lines * line_size) as u32)),
    );
}

fn benchmark_simple_pipe_with_many_writers(c: &mut Criterion) {
    let num_lines: usize = 10_000;
    let line_size: usize = 100;
    let num_writers: usize = 10;

    let in_addr = next_addr();
    let out_addr = next_addr();

    c.bench(
        "pipe_with_many_writers",
        Benchmark::new("pipe_with_many_writers", move |b| {
            b.iter_with_setup(
                || {
                    let mut topology = config::Config::empty();
                    topology.add_source(
                        "in",
                        sources::tcp::TcpConfig {
                            address: in_addr,
                            max_length: 102400,
                        },
                    );
                    topology.add_sink(
                        "out",
                        &["in"],
                        sinks::splunk::TcpSinkConfig { address: out_addr },
                    );
                    let (server, trigger, _healthchecks, _warnings) =
                        topology::build(topology).unwrap();

                    let mut rt = tokio::runtime::Runtime::new().unwrap();

                    let output_lines = count_lines(&out_addr, &rt.executor());

                    rt.spawn(server);
                    while let Err(_) = std::net::TcpStream::connect(in_addr) {}

                    (rt, trigger, output_lines)
                },
                |(mut rt, trigger, output_lines)| {
                    let sends = (0..num_writers)
                        .map(|_| {
                            let send = send_lines(in_addr, random_lines(line_size).take(num_lines));
                            futures::sync::oneshot::spawn(send, &rt.executor())
                        })
                        .collect::<Vec<_>>();

                    rt.block_on(future::join_all(sends)).unwrap();

                    drop(trigger);

                    rt.shutdown_on_idle().wait().unwrap();
                    assert_eq!(num_lines * num_writers, output_lines.wait().unwrap());
                },
            );
        })
        .sample_size(4)
        .noise_threshold(0.05)
        .throughput(Throughput::Bytes(
            (num_lines * line_size * num_writers) as u32,
        )),
    );
}

fn benchmark_interconnected(c: &mut Criterion) {
    let num_lines: usize = 100_000;
    let line_size: usize = 100;

    let in_addr1 = next_addr();
    let in_addr2 = next_addr();
    let out_addr1 = next_addr();
    let out_addr2 = next_addr();

    c.bench(
        "interconnected",
        Benchmark::new("interconnected", move |b| {
            b.iter_with_setup(
                || {
                    let mut topology = config::Config::empty();
                    topology.add_source(
                        "in1",
                        sources::tcp::TcpConfig {
                            address: in_addr1,
                            max_length: 102400,
                        },
                    );
                    topology.add_source(
                        "in2",
                        sources::tcp::TcpConfig {
                            address: in_addr2,
                            max_length: 102400,
                        },
                    );
                    topology.add_sink(
                        "out1",
                        &["in1", "in2"],
                        sinks::splunk::TcpSinkConfig { address: out_addr1 },
                    );
                    topology.add_sink(
                        "out2",
                        &["in1", "in2"],
                        sinks::splunk::TcpSinkConfig { address: out_addr2 },
                    );
                    let (server, trigger, _healthchecks, _warnings) =
                        topology::build(topology).unwrap();

                    let mut rt = tokio::runtime::Runtime::new().unwrap();

                    let output_lines1 = count_lines(&out_addr1, &rt.executor());
                    let output_lines2 = count_lines(&out_addr2, &rt.executor());

                    rt.spawn(server);
                    while let Err(_) = std::net::TcpStream::connect(in_addr1) {}
                    while let Err(_) = std::net::TcpStream::connect(in_addr2) {}

                    (rt, trigger, output_lines1, output_lines2)
                },
                |(mut rt, trigger, output_lines1, output_lines2)| {
                    let send1 = send_lines(in_addr1, random_lines(line_size).take(num_lines));
                    let send2 = send_lines(in_addr2, random_lines(line_size).take(num_lines));
                    let sends = vec![send1, send2];
                    rt.block_on(future::join_all(sends)).unwrap();

                    drop(trigger);

                    rt.shutdown_on_idle().wait().unwrap();
                    assert_eq!(num_lines * 2, output_lines1.wait().unwrap());
                    assert_eq!(num_lines * 2, output_lines2.wait().unwrap());
                },
            );
        })
        .sample_size(4)
        .noise_threshold(0.05)
        .throughput(Throughput::Bytes((num_lines * line_size * 2) as u32)),
    );
}

fn benchmark_transforms(c: &mut Criterion) {
    let num_lines: usize = 100_000;
    let line_size: usize = 100;

    let in_addr = next_addr();
    let out_addr = next_addr();

    c.bench(
        "transforms",
        Benchmark::new("transforms", move |b| {
            b.iter_with_setup(
                || {
                    let mut topology = config::Config::empty();
                    topology.add_source(
                        "in",
                        sources::tcp::TcpConfig {
                            address: in_addr,
                            max_length: 102400,
                        },
                    );
                    topology.add_transform(
                        "parser",
                        &["in"],
                        transforms::RegexParserConfig {
                            regex: r"status=(?P<status>\d+)".to_string(),
                        },
                    );
                    topology.add_transform(
                        "filter",
                        &["parser"],
                        transforms::FieldFilterConfig {
                            field: "status".to_string(),
                            value: "404".to_string(),
                        },
                    );
                    topology.add_sink(
                        "out",
                        &["filter"],
                        sinks::splunk::TcpSinkConfig { address: out_addr },
                    );
                    let (server, trigger, _healthchecks, _warnings) =
                        topology::build(topology).unwrap();
                    let mut rt = tokio::runtime::Runtime::new().unwrap();

                    let output_lines = count_lines(&out_addr, &rt.executor());

                    rt.spawn(server);
                    while let Err(_) = std::net::TcpStream::connect(in_addr) {}

                    (rt, trigger, output_lines)
                },
                |(mut rt, trigger, output_lines)| {
                    let send = send_lines(
                        in_addr,
                        random_lines(line_size)
                            .map(|l| l + "status=404")
                            .take(num_lines),
                    );
                    rt.block_on(send).unwrap();

                    drop(trigger);

                    rt.shutdown_on_idle().wait().unwrap();
                    assert_eq!(num_lines, output_lines.wait().unwrap());
                },
            );
        })
        .sample_size(4)
        .noise_threshold(0.05)
        .throughput(Throughput::Bytes(
            (num_lines * (line_size + "status=404".len())) as u32,
        )),
    );
}

fn benchmark_complex(c: &mut Criterion) {
    let num_lines: usize = 100_000;

    let in_addr1 = next_addr();
    let in_addr2 = next_addr();
    let out_addr_all = next_addr();
    let out_addr_sampled = next_addr();
    let out_addr_200 = next_addr();
    let out_addr_404 = next_addr();
    let out_addr_500 = next_addr();

    c.bench(
        "complex",
        Benchmark::new("complex", move |b| {
            b.iter_with_setup(
                || {
                    let mut topology = config::Config::empty();
                    topology.add_source(
                        "in1",
                        sources::tcp::TcpConfig {
                            address: in_addr1,
                            max_length: 102400,
                        },
                    );
                    topology.add_source(
                        "in2",
                        sources::tcp::TcpConfig {
                            address: in_addr2,
                            max_length: 102400,
                        },
                    );
                    topology.add_transform(
                        "parser",
                        &["in1", "in2"],
                        transforms::RegexParserConfig {
                            regex: r"status=(?P<status>\d+)".to_string(),
                        },
                    );
                    topology.add_transform(
                        "filter_200",
                        &["parser"],
                        transforms::FieldFilterConfig {
                            field: "status".to_string(),
                            value: "200".to_string(),
                        },
                    );
                    topology.add_transform(
                        "filter_404",
                        &["parser"],
                        transforms::FieldFilterConfig {
                            field: "status".to_string(),
                            value: "404".to_string(),
                        },
                    );
                    topology.add_transform(
                        "filter_500",
                        &["parser"],
                        transforms::FieldFilterConfig {
                            field: "status".to_string(),
                            value: "500".to_string(),
                        },
                    );
                    topology.add_transform(
                        "sampler",
                        &["parser"],
                        transforms::SamplerConfig {
                            rate: 10,
                            pass_list: vec![],
                        },
                    );
                    topology.add_sink(
                        "out_all",
                        &["parser"],
                        sinks::splunk::TcpSinkConfig {
                            address: out_addr_all,
                        },
                    );
                    topology.add_sink(
                        "out_sampled",
                        &["sampler"],
                        sinks::splunk::TcpSinkConfig {
                            address: out_addr_sampled,
                        },
                    );
                    topology.add_sink(
                        "out_200",
                        &["filter_200"],
                        sinks::splunk::TcpSinkConfig {
                            address: out_addr_200,
                        },
                    );
                    topology.add_sink(
                        "out_404",
                        &["filter_404"],
                        sinks::splunk::TcpSinkConfig {
                            address: out_addr_404,
                        },
                    );
                    topology.add_sink(
                        "out_500",
                        &["filter_500"],
                        sinks::splunk::TcpSinkConfig {
                            address: out_addr_500,
                        },
                    );
                    let (server, trigger, _healthchecks, _warnings) =
                        topology::build(topology).unwrap();
                    let mut rt = tokio::runtime::Runtime::new().unwrap();

                    let output_lines_all = count_lines(&out_addr_all, &rt.executor());
                    let output_lines_sampled = count_lines(&out_addr_sampled, &rt.executor());
                    let output_lines_200 = count_lines(&out_addr_200, &rt.executor());
                    let output_lines_404 = count_lines(&out_addr_404, &rt.executor());
                    let output_lines_500 = count_lines(&out_addr_500, &rt.executor());

                    rt.spawn(server);
                    while let Err(_) = std::net::TcpStream::connect(in_addr1) {}
                    while let Err(_) = std::net::TcpStream::connect(in_addr2) {}

                    (
                        rt,
                        trigger,
                        output_lines_all,
                        output_lines_sampled,
                        output_lines_200,
                        output_lines_404,
                        output_lines_500,
                    )
                },
                |(
                    mut rt,
                    trigger,
                    output_lines_all,
                    output_lines_sampled,
                    output_lines_200,
                    output_lines_404,
                    output_lines_500,
                )| {
                    use rand::{rngs::SmallRng, thread_rng, Rng, SeedableRng};

                    // One sender generates pure random lines
                    let send1 = send_lines(in_addr1, random_lines(100).take(num_lines));
                    let send1 = futures::sync::oneshot::spawn(send1, &rt.executor());

                    // The other includes either status=200 or status=404
                    let mut rng = SmallRng::from_rng(thread_rng()).unwrap();
                    let send2 = send_lines(
                        in_addr2,
                        random_lines(100)
                            .map(move |mut l| {
                                let status = if rng.gen_bool(0.5) { "200" } else { "404" };
                                l += "status=";
                                l += status;
                                l
                            })
                            .take(num_lines),
                    );
                    let send2 = futures::sync::oneshot::spawn(send2, &rt.executor());
                    let sends = vec![send1, send2];
                    rt.block_on(future::join_all(sends)).unwrap();

                    drop(trigger);

                    rt.shutdown_on_idle().wait().unwrap();

                    let output_lines_all = output_lines_all.wait().unwrap();
                    let output_lines_sampled = output_lines_sampled.wait().unwrap();
                    let output_lines_200 = output_lines_200.wait().unwrap();
                    let output_lines_404 = output_lines_404.wait().unwrap();
                    let output_lines_500 = output_lines_500.wait().unwrap();

                    assert_eq!(output_lines_all, num_lines * 2);
                    assert_relative_eq!(
                        output_lines_sampled as f32 / num_lines as f32,
                        0.2,
                        epsilon = 0.01
                    );
                    assert!(output_lines_200 > 0);
                    assert!(output_lines_404 > 0);
                    assert_eq!(output_lines_200 + output_lines_404, num_lines);
                    assert_eq!(output_lines_500, 0);
                },
            );
        })
        .sample_size(2),
    );
}

criterion_group!(
    benches,
    benchmark_simple_pipe,
    benchmark_simple_pipe_with_tiny_lines,
    benchmark_simple_pipe_with_huge_lines,
    benchmark_simple_pipe_with_many_writers,
    benchmark_interconnected,
    benchmark_transforms,
    benchmark_complex,
);
criterion_main!(benches);

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
