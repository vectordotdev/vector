use crate::common::AlwaysFail;
use crate::common::AlwaysPass;
use criterion::{
    criterion_group, measurement::WallTime, BatchSize, BenchmarkGroup, Criterion, SamplingMode,
    Throughput,
};
use std::time::Duration;
use vector::transforms::filter::Filter;
use vector::transforms::FunctionTransform;
use vector_core::event::{Event, LogEvent};

///
/// `Filter::transform` benchmarks
///
/// This benchmark examines the `transform` of `Filter`, demonstrating that its
/// performance is bounded entirely by that of the `Condition`. The two cases
/// below, `always_pass` and `always_fail` use `common::AlwaysPass` and
/// `common::AlwaysFail` as the interior condition of the filter.
///
fn filter(c: &mut Criterion) {
    let mut group: BenchmarkGroup<WallTime> =
        c.benchmark_group("vector::transforms::filter::Filter");
    group.sampling_mode(SamplingMode::Auto);

    group.throughput(Throughput::Elements(1));
    group.bench_function("transform/always_fail", |b| {
        b.iter_batched(
            || {
                let filter = Filter::new(Box::new(AlwaysFail));
                let output = Vec::with_capacity(4); // arbitrary constant larger
                                                    // than output
                let event = Event::Log(LogEvent::default());
                (filter, output, event)
            },
            |(mut filter, mut output, event)| filter.transform(&mut output, event),
            BatchSize::SmallInput,
        )
    });
    group.bench_function("transform/always_pass", |b| {
        b.iter_batched(
            || {
                let filter = Filter::new(Box::new(AlwaysPass));
                let output = Vec::with_capacity(4); // arbitrary constant larger
                                                    // than output
                let event = Event::Log(LogEvent::default());
                (filter, output, event)
            },
            |(mut filter, mut output, event)| filter.transform(&mut output, event),
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(60))
        .confidence_level(0.99)
        .nresamples(250_000)
        .sample_size(250);
    targets = filter
);
