use criterion::BenchmarkId;
use criterion::{
    criterion_group, measurement::WallTime, BatchSize, BenchmarkGroup, Criterion, SamplingMode,
};
use vector_core::event::{LogEvent, Value};

fn contains(c: &mut Criterion) {
    let mut group: BenchmarkGroup<WallTime> =
        c.benchmark_group("vector_core::event::LogEvent::contains");
    group.sampling_mode(SamplingMode::Auto);

    group.bench_function(BenchmarkId::new("contains", "does"), |b| {
        b.iter_batched(
            || {
                let mut log = LogEvent::default();
                log.insert_flat("a".to_string(), Value::Null);
                let query = "a";
                (log, query)
            },
            |(log, query)| {
                log.contains(query);
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function(BenchmarkId::new("contains", "does_not"), |b| {
        b.iter_batched(
            || {
                let log = LogEvent::default();
                let query = "a";
                (log, query)
            },
            |(log, query)| {
                log.contains(query);
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function(BenchmarkId::new("contains", "deep_does_not"), |b| {
        b.iter_batched(
            || {
                let log = LogEvent::default();
                let query = "a.b.c.d.e";
                (log, query)
            },
            |(log, query)| {
                log.contains(query);
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        // degree of noise to ignore in measurements, here 1%
        .noise_threshold(0.01)
        // likelihood of noise registering as difference, here 5%
        .significance_level(0.05)
        // likelihood of capturing the true runtime, here 95%
        .confidence_level(0.95)
        // total number of bootstrap resamples, higher is less noisy but slower
        .nresamples(100_000)
        // total samples to collect within the set measurement time
        .sample_size(200);
    targets = contains
);
