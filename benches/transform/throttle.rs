use std::{collections::HashMap, time::Duration};

use criterion::{
    BatchSize, BenchmarkGroup, Criterion, SamplingMode, Throughput, criterion_group,
    measurement::WallTime,
};
use governor::clock;
use tokio::runtime::Runtime;
use vector::{
    config::{DataType, TransformContext, TransformOutput},
    transforms::{
        SyncTransform, TransformOutputsBuf,
        throttle::{
            DROPPED,
            config::ThrottleConfig,
            transform::Throttle,
        },
    },
};
use vector_lib::event::{Event, LogEvent};

struct Payload {
    transform: Box<dyn SyncTransform>,
    output: TransformOutputsBuf,
    events: Vec<Event>,
    rt: Runtime,
}

fn make_runtime() -> Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn make_output_buf(reroute_dropped: bool) -> TransformOutputsBuf {
    let mut outputs = vec![TransformOutput::new(DataType::Log, HashMap::new())];
    if reroute_dropped {
        outputs.push(TransformOutput::new(DataType::Log, HashMap::new()).with_port(DROPPED));
    }
    TransformOutputsBuf::new_with_capacity(outputs, 1)
}

fn make_events(total_events: usize, message_prefix: &str) -> Vec<Event> {
    (0..total_events)
        .map(|i| {
            let mut log = LogEvent::default();
            log.insert("message", format!("{message_prefix} {i}"));
            Event::Log(log)
        })
        .collect()
}

fn make_keyed_events(total_events: usize, message_prefix: &str, num_keys: usize) -> Vec<Event> {
    (0..total_events)
        .map(|i| {
            let mut log = LogEvent::default();
            log.insert("message", format!("{message_prefix} {i}"));
            log.insert("service", format!("svc-{}", i % num_keys));
            Event::Log(log)
        })
        .collect()
}

fn build_throttle(rt: &Runtime, config: &ThrottleConfig) -> Box<dyn SyncTransform> {
    let _guard = rt.enter();
    let throttle =
        Throttle::new(config, &TransformContext::default(), clock::MonotonicClock).unwrap();
    Box::new(throttle.into_sync_transform())
}

fn setup_events_only(total_events: usize, threshold: u32) -> Payload {
    let rt = make_runtime();
    let config = toml::from_str::<ThrottleConfig>(&format!(
        "threshold = {threshold}\nwindow_secs = 60\n"
    ))
    .unwrap();
    let transform = build_throttle(&rt, &config);

    Payload {
        transform,
        output: make_output_buf(false),
        events: make_events(total_events, "event"),
        rt,
    }
}

fn setup_json_bytes(total_events: usize, byte_threshold: u32) -> Payload {
    let rt = make_runtime();
    let config = toml::from_str::<ThrottleConfig>(&format!(
        "window_secs = 60\n\n[threshold]\njson_bytes = {byte_threshold}\n"
    ))
    .unwrap();
    let transform = build_throttle(&rt, &config);

    Payload {
        transform,
        output: make_output_buf(false),
        events: make_events(total_events, "event payload data for benchmark"),
        rt,
    }
}

fn setup_vrl_tokens(total_events: usize) -> Payload {
    let rt = make_runtime();
    let config = toml::from_str::<ThrottleConfig>(
        "window_secs = 60\n\n[threshold]\njson_bytes = 1000000\ntokens = 'strlen(string!(.message))'\n",
    )
    .unwrap();
    let transform = build_throttle(&rt, &config);

    Payload {
        transform,
        output: make_output_buf(false),
        events: make_events(total_events, "event payload data for VRL bench"),
        rt,
    }
}

fn setup_events_and_bytes(total_events: usize) -> Payload {
    let rt = make_runtime();
    let config = toml::from_str::<ThrottleConfig>(
        "window_secs = 60\n\n[threshold]\nevents = 10000\njson_bytes = 1000000\n",
    )
    .unwrap();
    let transform = build_throttle(&rt, &config);

    Payload {
        transform,
        output: make_output_buf(false),
        events: make_events(total_events, "event payload data"),
        rt,
    }
}

fn setup_all_three(total_events: usize) -> Payload {
    let rt = make_runtime();
    let config = toml::from_str::<ThrottleConfig>(
        "window_secs = 60\n\n[threshold]\nevents = 10000\njson_bytes = 1000000\ntokens = 'strlen(string!(.message))'\n",
    )
    .unwrap();
    let transform = build_throttle(&rt, &config);

    Payload {
        transform,
        output: make_output_buf(false),
        events: make_events(total_events, "event payload data"),
        rt,
    }
}

fn setup_with_dropped_port(total_events: usize, threshold: u32) -> Payload {
    let rt = make_runtime();
    let config = toml::from_str::<ThrottleConfig>(&format!(
        "threshold = {threshold}\nwindow_secs = 60\nreroute_dropped = true\n"
    ))
    .unwrap();
    let transform = build_throttle(&rt, &config);

    Payload {
        transform,
        output: make_output_buf(true),
        events: make_events(total_events, "event"),
        rt,
    }
}

fn setup_high_cardinality_keys(total_events: usize) -> Payload {
    let rt = make_runtime();
    let config = toml::from_str::<ThrottleConfig>(
        "threshold = 100\nwindow_secs = 60\nkey_field = \"{{ service }}\"\n",
    )
    .unwrap();
    let transform = build_throttle(&rt, &config);

    Payload {
        transform,
        output: make_output_buf(false),
        events: make_keyed_events(total_events, "event", 100),
        rt,
    }
}

fn setup_metrics_variant(
    total_events: usize,
    emit_events_discarded_per_key: bool,
    emit_detailed_metrics: bool,
    num_keys: usize,
    threshold_config: &str,
) -> Payload {
    let rt = make_runtime();
    let config_str = format!(
        "window_secs = 60\nkey_field = \"{{{{ service }}}}\"\n{threshold_config}\n\n\
         [internal_metrics]\nemit_events_discarded_per_key = {emit_events_discarded_per_key}\n\
         emit_detailed_metrics = {emit_detailed_metrics}\n"
    );
    let config = toml::from_str::<ThrottleConfig>(&config_str).unwrap();
    let transform = build_throttle(&rt, &config);

    Payload {
        transform,
        output: make_output_buf(false),
        events: make_keyed_events(total_events, "event payload data for benchmark", num_keys),
        rt,
    }
}

/// Measurement function that ensures a Tokio runtime context is available.
/// The ThrottleSyncTransform lazily creates rate limiters on the first call
/// to `transform()`, which requires a Tokio runtime for spawning the
/// background key-flush task.
fn measurement(payload: Payload) {
    let _guard = payload.rt.enter();
    let mut transform = payload.transform;
    let mut output = payload.output;

    for event in payload.events {
        transform.transform(event, &mut output);
    }
}

fn throttle(c: &mut Criterion) {
    let mut group: BenchmarkGroup<WallTime> =
        c.benchmark_group("vector::transforms::throttle::Throttle");
    group.sampling_mode(SamplingMode::Auto);

    let total_events = 1024;
    group.throughput(Throughput::Elements(total_events as u64));

    // A. Throughput benchmarks
    group.bench_function("events_only/under_limit", |b| {
        b.iter_batched(
            || setup_events_only(total_events, 10000),
            measurement,
            BatchSize::SmallInput,
        )
    });

    group.bench_function("events_only/over_limit", |b| {
        b.iter_batched(
            || setup_events_only(total_events, 100),
            measurement,
            BatchSize::SmallInput,
        )
    });

    group.bench_function("json_bytes_only", |b| {
        b.iter_batched(
            || setup_json_bytes(total_events, 1_000_000),
            measurement,
            BatchSize::SmallInput,
        )
    });

    group.bench_function("vrl_tokens", |b| {
        b.iter_batched(
            || setup_vrl_tokens(total_events),
            measurement,
            BatchSize::SmallInput,
        )
    });

    group.bench_function("events_and_bytes", |b| {
        b.iter_batched(
            || setup_events_and_bytes(total_events),
            measurement,
            BatchSize::SmallInput,
        )
    });

    group.bench_function("all_three_thresholds", |b| {
        b.iter_batched(
            || setup_all_three(total_events),
            measurement,
            BatchSize::SmallInput,
        )
    });

    group.bench_function("with_dropped_port", |b| {
        b.iter_batched(
            || setup_with_dropped_port(total_events, 100),
            measurement,
            BatchSize::SmallInput,
        )
    });

    group.bench_function("high_cardinality_keys", |b| {
        b.iter_batched(
            || setup_high_cardinality_keys(total_events),
            measurement,
            BatchSize::SmallInput,
        )
    });

    group.finish();

    // B. Metrics impact benchmarks — measure per-event cost of metrics flags
    let mut metrics_group: BenchmarkGroup<WallTime> =
        c.benchmark_group("vector::transforms::throttle::metrics_overhead");
    metrics_group.sampling_mode(SamplingMode::Auto);
    metrics_group.throughput(Throughput::Elements(total_events as u64));

    let threshold = "threshold = 10000";

    metrics_group.bench_function("metrics_both_off", |b| {
        b.iter_batched(
            || setup_metrics_variant(total_events, false, false, 100, threshold),
            measurement,
            BatchSize::SmallInput,
        )
    });

    metrics_group.bench_function("metrics_legacy_only", |b| {
        b.iter_batched(
            || setup_metrics_variant(total_events, true, false, 100, threshold),
            measurement,
            BatchSize::SmallInput,
        )
    });

    metrics_group.bench_function("metrics_detailed_only", |b| {
        b.iter_batched(
            || setup_metrics_variant(total_events, false, true, 100, threshold),
            measurement,
            BatchSize::SmallInput,
        )
    });

    metrics_group.bench_function("metrics_both_on", |b| {
        b.iter_batched(
            || setup_metrics_variant(total_events, true, true, 100, threshold),
            measurement,
            BatchSize::SmallInput,
        )
    });

    metrics_group.bench_function("metrics_detailed_high_cardinality", |b| {
        b.iter_batched(
            || setup_metrics_variant(total_events, false, true, 10_000, threshold),
            measurement,
            BatchSize::SmallInput,
        )
    });

    let multi_threshold = "[threshold]\nevents = 10000\njson_bytes = 1000000";

    metrics_group.bench_function("metrics_detailed_all_thresholds", |b| {
        b.iter_batched(
            || setup_metrics_variant(total_events, false, true, 100, multi_threshold),
            measurement,
            BatchSize::SmallInput,
        )
    });

    metrics_group.finish();

    // C. Key cardinality scaling — measure throughput as unique keys grow
    let mut cardinality_group: BenchmarkGroup<WallTime> =
        c.benchmark_group("vector::transforms::throttle::key_cardinality");
    cardinality_group.sampling_mode(SamplingMode::Auto);
    cardinality_group.throughput(Throughput::Elements(total_events as u64));

    for num_keys in [10, 100, 1_000] {
        // Events-only with N keys
        cardinality_group.bench_function(&format!("events_only/{num_keys}_keys"), |b| {
            b.iter_batched(
                || {
                    let rt = make_runtime();
                    let config = toml::from_str::<ThrottleConfig>(
                        "threshold = 10000\nwindow_secs = 60\nkey_field = \"{{ service }}\"\n",
                    )
                    .unwrap();
                    let transform = build_throttle(&rt, &config);
                    Payload {
                        transform,
                        output: make_output_buf(false),
                        events: make_keyed_events(total_events, "event", num_keys),
                        rt,
                    }
                },
                measurement,
                BatchSize::SmallInput,
            )
        });

        // Events + bytes with N keys
        cardinality_group.bench_function(&format!("events_and_bytes/{num_keys}_keys"), |b| {
            b.iter_batched(
                || {
                    let rt = make_runtime();
                    let config = toml::from_str::<ThrottleConfig>(
                        "window_secs = 60\nkey_field = \"{{ service }}\"\n\n[threshold]\nevents = 10000\njson_bytes = 1000000\n",
                    )
                    .unwrap();
                    let transform = build_throttle(&rt, &config);
                    Payload {
                        transform,
                        output: make_output_buf(false),
                        events: make_keyed_events(total_events, "event payload data", num_keys),
                        rt,
                    }
                },
                measurement,
                BatchSize::SmallInput,
            )
        });

        // All three thresholds with N keys
        cardinality_group.bench_function(&format!("all_three/{num_keys}_keys"), |b| {
            b.iter_batched(
                || {
                    let rt = make_runtime();
                    let config = toml::from_str::<ThrottleConfig>(
                        "window_secs = 60\nkey_field = \"{{ service }}\"\n\n[threshold]\nevents = 10000\njson_bytes = 1000000\ntokens = 'strlen(string!(.message))'\n",
                    )
                    .unwrap();
                    let transform = build_throttle(&rt, &config);
                    Payload {
                        transform,
                        output: make_output_buf(false),
                        events: make_keyed_events(total_events, "event payload data", num_keys),
                        rt,
                    }
                },
                measurement,
                BatchSize::SmallInput,
            )
        });
    }

    cardinality_group.finish();
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(5))
        .measurement_time(Duration::from_secs(30))
        .noise_threshold(0.02)
        .significance_level(0.05)
        .confidence_level(0.95)
        .nresamples(100_000)
        .sample_size(200);
    targets = throttle
);
