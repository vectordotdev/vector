use criterion::{criterion_group, BatchSize, Criterion, SamplingMode, Throughput};
use tempfile::tempdir;
use vector::test_util::{
    next_addr, random_lines, runtime, send_lines, start_topology, wait_for_tcp, CountReceiver,
};
use vector::{buffers::BufferConfig, config, sinks, sources};

fn benchmark_buffers(c: &mut Criterion) {
    let num_lines: usize = 10_000;
    let line_size: usize = 100;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut group = c.benchmark_group("buffers");
    group.throughput(Throughput::Bytes((num_lines * line_size) as u64));
    group.sampling_mode(SamplingMode::Flat);

    group.bench_function("in-memory", |b| {
        b.iter_batched(
            || {
                let mut config = config::Config::builder();
                config.add_source(
                    "in",
                    sources::socket::SocketConfig::make_basic_tcp_config(in_addr),
                );
                config.add_sink(
                    "out",
                    &["in"],
                    sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
                );
                config.sinks["out"].buffer = BufferConfig::Memory {
                    max_events: 100,
                    when_full: Default::default(),
                };

                let mut rt = runtime();
                let (output_lines, topology) = rt.block_on(async move {
                    let output_lines = CountReceiver::receive_lines(out_addr);
                    let (topology, _crash) = start_topology(config.build().unwrap(), false).await;
                    wait_for_tcp(in_addr).await;
                    (output_lines, topology)
                });

                (rt, topology, output_lines)
            },
            |(mut rt, topology, output_lines)| {
                rt.block_on(async move {
                    let lines = random_lines(line_size).take(num_lines);
                    send_lines(in_addr, lines).await.unwrap();

                    topology.stop().await;

                    let output_lines = output_lines.await;

                    debug_assert_eq!(num_lines, output_lines.len());

                    output_lines
                });
            },
            BatchSize::PerIteration,
        );
    });

    group.bench_function("on-disk", |b| {
        b.iter_batched(
            || {
                let data_dir = tempdir().unwrap();

                let mut config = config::Config::builder();
                config.add_source(
                    "in",
                    sources::socket::SocketConfig::make_basic_tcp_config(in_addr),
                );
                config.add_sink(
                    "out",
                    &["in"],
                    sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
                );
                config.sinks["out"].buffer = BufferConfig::Disk {
                    max_size: 1_000_000,
                    when_full: Default::default(),
                };
                config.global.data_dir = Some(data_dir.path().to_path_buf());
                let mut rt = runtime();
                let (output_lines, topology) = rt.block_on(async move {
                    let output_lines = CountReceiver::receive_lines(out_addr);
                    let (topology, _crash) = start_topology(config.build().unwrap(), false).await;
                    wait_for_tcp(in_addr).await;
                    (output_lines, topology)
                });
                (rt, topology, output_lines)
            },
            |(mut rt, topology, output_lines)| {
                rt.block_on(async move {
                    let lines = random_lines(line_size).take(num_lines);
                    send_lines(in_addr, lines).await.unwrap();
                    topology.stop().await;

                    // TODO: shutdown after flush
                    // assert_eq!(num_lines, output_lines.await.len());
                    output_lines.await
                });
            },
            BatchSize::PerIteration,
        );
    });

    // TODO(jesse): reenable
    // This benchmark hangs in CI sometimes
    // https://github.com/timberio/vector/issues/5389
    //
    //group.bench_function("low-limit-on-disk", |b| {
    //b.iter_batched(
    //|| {
    //let data_dir = tempdir().unwrap();

    //let mut config = config::Config::builder();
    //config.add_source(
    //"in",
    //sources::socket::SocketConfig::make_basic_tcp_config(in_addr),
    //);
    //config.add_sink(
    //"out",
    //&["in"],
    //sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
    //);
    //config.sinks["out"].buffer = BufferConfig::Disk {
    //max_size: 10_000,
    //when_full: Default::default(),
    //};
    //config.global.data_dir = Some(data_dir.path().to_path_buf());
    //let mut rt = runtime();
    //let (output_lines, topology) = rt.block_on(async move {
    //let output_lines = CountReceiver::receive_lines(out_addr);
    //let (topology, _crash) = start_topology(config.build().unwrap(), false).await;
    //wait_for_tcp(in_addr).await;
    //(output_lines, topology)
    //});
    //(rt, topology, output_lines)
    //},
    //|(mut rt, topology, output_lines)| {
    //rt.block_on(async move {
    //let lines = random_lines(line_size).take(num_lines);
    //send_lines(in_addr, lines).await.unwrap();
    //topology.stop().await;

    //// TODO: shutdown after flush
    //// assert_eq!(num_lines, output_lines.await.len());
    //output_lines.await
    //});
    //},
    //BatchSize::PerIteration,
    //);
    //});
}

criterion_group!(
    name = benches;
    // encapsulates CI noise we saw in
    // https://github.com/timberio/vector/issues/5394
    config = Criterion::default().noise_threshold(0.05);
    targets = benchmark_buffers
);
