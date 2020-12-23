//! This mod contains the benchmarks for metrics.
//! Due to how metrics are set up, we need isolated process environments to
//! test with metrics core on and off, so the implemntation is shared but has
//! two separate entrypoits.
//!
//! # Usage
//!
//! To run these benches, use the following commands:
//!
//!     cargo bench --no-default-features --features "metrics-benches" --bench metrics_on
//!     cargo bench --no-default-features --features "metrics-benches" --bench metrics_off
//!
//! These will run benches with metrics core system on and off respectively.

use criterion::{BatchSize, Criterion, SamplingMode, Throughput};
use futures::{compat::Future01CompatExt, future, stream, StreamExt};
use metrics::counter;
use tracing::{span, Level};
use vector::{
    config, sinks, sources,
    test_util::{
        next_addr, random_lines, runtime, send_lines, start_topology, wait_for_tcp, CountReceiver,
    },
};

#[inline]
fn disable_metrics() {
    std::env::set_var("DISABLE_INTERNAL_METRICS_CORE", "true");
    std::env::set_var("DISABLE_INTERNAL_METRICS_TRACING_LAYER", "true");
}

#[inline]
fn boot() {
    vector::trace::init(false, false, "warn");
    vector::metrics::init().expect("metrics initialization failed");
}

/// Due to the nature of how metrics and tracing systems are set up, we need
/// separate process spaces to measure with and without them being enabled.
#[inline]
pub fn benchmark(c: &mut Criterion, metrics_enabled: bool) {
    if !metrics_enabled {
        disable_metrics();
    }
    boot();
    assert_eq!(
        vector::metrics::get_controller().is_ok(),
        metrics_enabled,
        "the presence of a controller must correspond to whether metrics are on or off"
    );

    let bench_name = if metrics_enabled {
        "metrics_on"
    } else {
        "metrics_off"
    };

    bench_topology(c, bench_name);
    bench_micro(c, bench_name);
}

/// Based on the topology benchmarks.
/// I'm pretty sure criterion used incorrectly here - the bench loops generates
/// it's own input, and taps into random source and networking - this must not
/// be working right, as criterion assumptions about the benchmark loop
/// iteration payload are broken here.
#[inline]
fn bench_topology(c: &mut Criterion, bench_name: &'static str) {
    let num_lines: usize = 10_000;
    let line_size: usize = 100;
    let num_writers = 2;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut group = c.benchmark_group(format!("{}/{}", bench_name, "topology"));
    group.sampling_mode(SamplingMode::Flat);

    group.throughput(Throughput::Bytes(
        (num_lines * line_size * num_writers) as u64,
    ));
    group.bench_function("tcp_socket", |b| {
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
                    let sends = stream::iter(0..num_writers)
                        .map(|_| {
                            let lines = random_lines(line_size).take(num_lines);
                            send_lines(in_addr, lines)
                        })
                        .collect::<Vec<_>>()
                        .await;
                    future::try_join_all(sends).await.unwrap();

                    topology.stop().compat().await.unwrap();

                    let output_lines = output_lines.await;

                    debug_assert_eq!(num_lines * num_writers, output_lines.len());

                    output_lines
                });
            },
            BatchSize::PerIteration,
        );
    });

    group.finish();
}

/// Here we perform some microbenchmarks on the metrics.
#[inline]
fn bench_micro(c: &mut Criterion, bench_name: &'static str) {
    let mut group = c.benchmark_group(format!("{}/{}", bench_name, "micro"));
    group.bench_function("bare_counter", |b| {
        b.iter(|| {
            counter!("test", 1);
        });
    });
    group.bench_function("bare_counter_with_static_labels", |b| {
        b.iter(|| {
            counter!("test", 1, "my key" => "my value");
        });
    });
    group.bench_function("bare_counter_with_dynamic_labels", |b| {
        b.iter_batched(
            || "my value".to_owned(),
            |my_value| {
                counter!("test", 1, "my key" => my_value);
            },
            BatchSize::SmallInput,
        );
    });
    // A span that's not even entered.
    group.bench_function("ununsed_span", |b| {
        b.iter_batched_ref(
            || span!(Level::ERROR, "my span"),
            |_span| {
                counter!("test", 1);
            },
            BatchSize::SmallInput,
        );
    });
    // A span that's entered but without a counter invocation.
    group.bench_function("span_enter_without_counter", |b| {
        b.iter_batched_ref(
            || span!(Level::ERROR, "my span"),
            |span| {
                let _enter = span.enter();
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("span_no_labels", |b| {
        b.iter_batched_ref(
            || span!(Level::ERROR, "my span"),
            |span| {
                let _enter = span.enter();
                counter!("test", 1);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("span_with_1_static_label", |b| {
        b.iter_batched_ref(
            || span!(Level::ERROR, "my span", "my key" = "my value"),
            |span| {
                let _enter = span.enter();
                counter!("test", 1);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("span_with_2_static_labels", |b| {
        b.iter_batched_ref(
            || {
                span!(
                    Level::ERROR,
                    "my span",
                    "my key 1" = "my value 1",
                    "my key 2" = "my value 2"
                )
            },
            |span| {
                let _enter = span.enter();
                counter!("test", 1);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("span_with_1_dynamic_label", |b| {
        b.iter_batched_ref(
            || {
                let my_value = "my value".to_owned();
                span!(Level::ERROR, "my span", "my key" = %my_value)
            },
            |span| {
                let _enter = span.enter();
                counter!("test", 1);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("span_with_2_dynamic_labels", |b| {
        b.iter_batched_ref(
            || {
                let my_value_1 = "my value 1".to_owned();
                let my_value_2 = "my value 2".to_owned();
                span!(
                    Level::ERROR,
                    "my span",
                    "my key 1" = %my_value_1,
                    "my key 2" = %my_value_2
                )
            },
            |span| {
                let _enter = span.enter();
                counter!("test", 1);
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
}
