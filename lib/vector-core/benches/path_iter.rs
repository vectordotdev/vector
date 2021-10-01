use criterion::{
    criterion_group, criterion_main, measurement::WallTime, BenchmarkGroup, Criterion, SamplingMode,
};
use vector_core::event::PathIter;

fn path_iter(c: &mut Criterion) {
    let mut group: BenchmarkGroup<WallTime> = c.benchmark_group("vector_core::event::util::log");
    group.sampling_mode(SamplingMode::Auto);

    group.bench_function("PathIter (flat)", move |b| {
        b.iter_with_large_drop(|| {
            let iter = PathIter::new("message");
            iter.collect::<Vec<_>>()
        })
    });

    group.bench_function("PathIter (nested)", move |b| {
        b.iter_with_large_drop(|| {
            let iter = PathIter::new("obj.message");
            iter.collect::<Vec<_>>()
        })
    });

    group.bench_function("PathIter (nested array)", move |b| {
        b.iter_with_large_drop(|| {
            let iter = PathIter::new("obj.messages[2]");
            iter.collect::<Vec<_>>()
        })
    });

    group.bench_function("PathIter (nested escaped)", move |b| {
        b.iter_with_large_drop(|| {
            let iter = PathIter::new("obj.\\messages[]\\");
            iter.collect::<Vec<_>>()
        })
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
        .sample_size(150);
    targets = path_iter
);
criterion_main!(benches);
