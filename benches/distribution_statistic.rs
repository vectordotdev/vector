use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use rand::{
    distributions::{Distribution, Uniform},
    seq::SliceRandom,
};
use vector::{event::metric::Sample, sinks::util::statistic::DistributionStatistic};

fn generate_samples(mut size: u32, max_bin_count: u32) -> Vec<Sample> {
    let mut rng = rand::thread_rng();
    let range = Uniform::from(1..=max_bin_count);
    let mut value = 1.0;
    let mut samples = Vec::new();
    while size > 0 {
        let bin_count = u32::min(range.sample(&mut rng), size);
        samples.push(Sample {
            value,
            rate: bin_count,
        });
        size -= bin_count;
        value += 1.0;
    }
    samples.shuffle(&mut rng);
    samples
}

fn bench_statistic(c: &mut Criterion) {
    let mut group = c.benchmark_group("distribution_statistic");

    let sizes = [5, 10, 50, 100, 200, 500, 1000];
    for &size in &sizes {
        group.bench_function(format!("small-bin-{}", size), |b| {
            b.iter_batched(
                move || generate_samples(size, 3),
                |samples| {
                    DistributionStatistic::from_samples(&samples, &[0.5, 0.75, 0.9, 0.95, 0.99])
                },
                BatchSize::SmallInput,
            );
        });
    }

    let sizes = [50, 100, 200, 500, 1000];
    for &size in &sizes {
        group.bench_function(format!("large-bin-{}", size), |b| {
            b.iter_batched(
                move || generate_samples(size, 20),
                |samples| {
                    DistributionStatistic::from_samples(&samples, &[0.5, 0.75, 0.9, 0.95, 0.99])
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

criterion_group!(
    name = benches;
    config = Criterion::default().noise_threshold(0.1);
    targets = bench_statistic
);
criterion_main!(benches);
