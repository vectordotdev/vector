use std::time::Duration;

use criterion::{
    criterion_group, measurement::WallTime, BatchSize, BenchmarkGroup, Criterion, SamplingMode,
    Throughput,
};
use vector::{
    conditions::Condition,
    transforms::{filter::Filter, FunctionTransform, OutputBuffer},
};
use vector_lib::event::{Event, LogEvent};

struct Payload {
    filter: Filter,
    output: OutputBuffer,
    events: Vec<Event>,
}

fn setup(total_events: usize, condition: Condition) -> Payload {
    let filter = Filter::new(condition);
    let output = OutputBuffer::from(Vec::with_capacity(total_events));
    let events = vec![Event::Log(LogEvent::default()); total_events];
    Payload {
        filter,
        output,
        events,
    }
}

fn measurement(payload: Payload) {
    let mut filter = payload.filter;
    let mut output = payload.output;
    let events = payload.events;

    for event in events {
        filter.transform(&mut output, event)
    }
}

///
/// `Filter::transform` benchmarks
///
/// This benchmark examines the `transform` of `Filter`, demonstrating that its
/// performance is bounded entirely by that of the `Condition`. The two cases
/// below, `always_pass` and `always_fail` use `Condition::AlwaysPass` and
/// `Condition::AlwaysFail` as the interior condition of the filter.
///
fn filter(c: &mut Criterion) {
    let mut group: BenchmarkGroup<WallTime> =
        c.benchmark_group("vector::transforms::filter::Filter");
    group.sampling_mode(SamplingMode::Auto);

    let total_events = 1024; // arbitrary constant, the smaller the noisier
    group.throughput(Throughput::Elements(total_events as u64));
    group.bench_function("transform/always_fail", |b| {
        b.iter_batched(
            || setup(total_events, Condition::AlwaysFail),
            measurement,
            BatchSize::SmallInput,
        )
    });
    group.bench_function("transform/always_pass", |b| {
        b.iter_batched(
            || setup(total_events, Condition::AlwaysPass),
            measurement,
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(10))
        .measurement_time(Duration::from_secs(180))
        // degree of noise to ignore in measurements, here 1%
        .noise_threshold(0.01)
        // likelihood of noise registering as difference, here 5%
        .significance_level(0.05)
        // likelihood of capturing the true runtime, here 95%
        .confidence_level(0.95)
        // total number of bootstrap resamples, higher is less noisy but slower
        .nresamples(100_000)
        // total samples to collect within the set measurement time
        .sample_size(500);
    targets = filter
);
