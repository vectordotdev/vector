use std::time::Duration;

use criterion::{
    criterion_group, measurement::WallTime, BatchSize, BenchmarkGroup, Criterion, SamplingMode,
};
use vector_core::event::LogEvent;

fn rename_key_flat(c: &mut Criterion) {
    let mut group: BenchmarkGroup<WallTime> =
        c.benchmark_group("vector_core::event::log_event::LogEvent::rename_key_flat");
    group.sampling_mode(SamplingMode::Auto);

    group.bench_function("rename_flat_key (key is present)", move |b| {
        b.iter_batched(
            || {
                let mut log_event = LogEvent::default();
                log_event.insert("one", 1);
                log_event.insert("two", 2);
                log_event.insert("three", 3);
                log_event
            },
            |mut log_event| {
                log_event.rename_key_flat("one", "1");
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("rename_flat_key (key is NOT present)", move |b| {
        b.iter_batched(
            || {
                let mut log_event = LogEvent::default();
                log_event.insert("one", 1);
                log_event.insert("two", 2);
                log_event.insert("three", 3);
                log_event
            },
            |mut log_event| {
                log_event.rename_key_flat("four", "4");
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(5))
        .measurement_time(Duration::from_secs(120))
        // degree of noise to ignore in measurements, here 1%
        .noise_threshold(0.01)
        // likelihood of noise registering as difference, here 5%
        .significance_level(0.05)
        // likelihood of capturing the true runtime, here 95%
        .confidence_level(0.95)
        // total number of bootstrap resamples, higher is less noisy but slower
        .nresamples(100_000)
        // total samples to collect within the set measurement time
        .sample_size(150);
    targets = rename_key_flat
);
