mod dummy_service;
mod dummy_sink;
mod dummy_source;

use crate::dummy_source::StartBarrier;
use criterion::{
    criterion_group, criterion_main, BatchSize, Bencher, BenchmarkId, Criterion, Throughput,
};
use indoc::indoc;

use tracing::info;
use vector::config;
use vector::extra_context::ExtraContext;
use vector::test_util::runtime;
use vector::topology::RunningTopology;

criterion_group!(
    name = benches;
    // encapsulates inherent CI noise we saw in
    // https://github.com/vectordotdev/vector/issues/5394
    config = Criterion::default().noise_threshold(0.05);
    targets = benchmark_update_parsing
);

criterion_main! {
    benches,
}

fn run_benchmark(bench: &mut Bencher, params: &(usize, usize, usize, usize)) {
    let (concurrency, batch_count, batch_size, message_size) = params.clone();
    let config = format!(
        indoc! {r#"
            [sources.in]
             type = "dummy_source"
             client_concurrency = {}
             batch_count = {}
             batch_size = {}
             message_size = {}

            [sinks.out]
             type = "dummy_sink"
             inputs = ["in"]
        "#},
        concurrency, batch_count, batch_size, message_size
    );

    let rt = runtime();

    bench.iter_batched(
        || {
            let mut config = config::load_from_str(&config, config::Format::Toml)
                .expect(&format!("invalid TOML configuration: {}", &config));

            let barrier = StartBarrier::new(concurrency);

            let extra_context = ExtraContext::single_value(barrier.clone());

            let topology = rt.block_on(async move {
                config.healthchecks.set_require_healthy(false);
                let (topology, _) = RunningTopology::start_init_validated(config, extra_context)
                    .await
                    .unwrap();
                topology
            });
            rt.block_on(async {
                info!("Waiting for tasks to be ready");
                barrier.wait_ready().await;
                info!("All tasks ready!");
            });
            (barrier, topology)
        },
        |(barrier, topology)| {
            rt.block_on(async {
                info!("Starting!");
                barrier.wait_start().await;
                info!("Started!");
                topology.sources_finished().await;
                info!("Stopping!");
                topology.stop().await;
                info!("Stopped!");
            });
        },
        BatchSize::PerIteration,
    );
}

fn benchmark_update_parsing(c: &mut Criterion) {
    vector::test_util::trace_init();

    let mut group = c.benchmark_group("integration/performance");

    let concurrency_range = (50..=100).step_by(10);
    let batch_count_range = (25..=25).step_by(1);
    let batch_size_range = (10..=20).step_by(10);
    let message_size_range = (512..=2048).step_by(512);

    for concurrency in concurrency_range.clone() {
        for batch_count in batch_count_range.clone() {
            for batch_size in batch_size_range.clone() {
                for message_size in message_size_range.clone() {
                    let total_batches = concurrency * batch_count;
                    let total_messages = total_batches * batch_size;
                    let total_bytes = total_messages * message_size;
                    group.throughput(Throughput::Bytes(total_bytes as u64));

                    println!("Total batches: {}", total_batches);
                    println!("Total messages: {}", total_messages);
                    println!("Total megabytes: {}", total_bytes / 1024 / 1024);

                    group.sample_size(100);
                    let benchmark_id = format!("concurrency={concurrency}/batch_count={batch_count}/batch_size={batch_size}/msg_size={message_size}");
                    group.bench_with_input(
                        BenchmarkId::from_parameter(benchmark_id),
                        &(concurrency, batch_count, batch_size, message_size),
                        run_benchmark,
                    );
                }
            }
        }
    }

    group.finish();
}
