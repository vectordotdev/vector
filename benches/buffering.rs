#![allow(clippy::identity_conversion)]

use criterion::{criterion_group, criterion_main, Benchmark, Criterion, Throughput};

use tempfile::tempdir;
use vector::test_util::{
    block_on, count_receive, next_addr, send_lines, shutdown_on_idle, wait_for_tcp,
};
use vector::topology::{self, config};
use vector::{buffers::BufferConfig, runtime, sinks, sources};

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
                        sources::socket::SocketConfig::make_tcp_config(in_addr),
                    );
                    config.add_sink(
                        "out",
                        &["in"],
                        sinks::socket::SocketSinkConfig::make_basic_tcp_config(
                            out_addr.to_string(),
                        ),
                    );
                    config.sinks["out"].buffer = BufferConfig::Memory {
                        max_events: 100,
                        when_full: Default::default(),
                    };

                    let mut rt = runtime::Runtime::new().unwrap();

                    let output_lines = count_receive(&out_addr);

                    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
                    wait_for_tcp(in_addr);

                    (rt, topology, output_lines)
                },
                |(mut rt, topology, output_lines)| {
                    let send = send_lines(in_addr, random_lines(line_size).take(num_lines));
                    rt.block_on(send).unwrap();

                    block_on(topology.stop()).unwrap();

                    shutdown_on_idle(rt);
                    assert_eq!(num_lines, output_lines.wait());
                },
            );
        })
        .with_function("on-disk", move |b| {
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
                        sinks::socket::SocketSinkConfig::make_basic_tcp_config(
                            out_addr.to_string(),
                        ),
                    );
                    config.sinks["out"].buffer = BufferConfig::Disk {
                        max_size: 1_000_000,
                        when_full: Default::default(),
                    }
                    .into();
                    config.global.data_dir = Some(data_dir.clone());

                    let mut rt = runtime::Runtime::new().unwrap();

                    let output_lines = count_receive(&out_addr);

                    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
                    wait_for_tcp(in_addr);

                    (rt, topology, output_lines)
                },
                |(mut rt, topology, output_lines)| {
                    let send = send_lines(in_addr, random_lines(line_size).take(num_lines));
                    rt.block_on(send).unwrap();

                    block_on(topology.stop()).unwrap();

                    shutdown_on_idle(rt);
                    assert_eq!(num_lines, output_lines.wait());
                },
            );
        })
        .with_function("low-limit-on-disk", move |b| {
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
                        sinks::socket::SocketSinkConfig::make_basic_tcp_config(
                            out_addr.to_string(),
                        ),
                    );
                    config.sinks["out"].buffer = BufferConfig::Disk {
                        max_size: 10_000,
                        when_full: Default::default(),
                    };
                    config.global.data_dir = Some(data_dir2.clone());

                    let mut rt = runtime::Runtime::new().unwrap();

                    let output_lines = count_receive(&out_addr);

                    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
                    wait_for_tcp(in_addr);

                    (rt, topology, output_lines)
                },
                |(mut rt, topology, output_lines)| {
                    let send = send_lines(in_addr, random_lines(line_size).take(num_lines));
                    rt.block_on(send).unwrap();

                    block_on(topology.stop()).unwrap();

                    shutdown_on_idle(rt);
                    assert_eq!(num_lines, output_lines.wait());
                },
            );
        })
        .sample_size(10)
        .noise_threshold(0.05)
        .throughput(Throughput::Bytes((num_lines * line_size) as u64)),
    );
}

criterion_group!(buffers, benchmark_buffers);
criterion_main!(buffers);

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
