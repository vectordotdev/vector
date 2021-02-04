use criterion::{criterion_group, criterion_main, BatchSize, Criterion};

use rand::distributions::Distribution;
use rand::distributions::Uniform;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand::rngs::SmallRng;

use vector::event::metric::Sample;
use vector::sinks::util::statistic::DistributionStatistic;

fn generate_samples(mut size: u32) -> Vec<Sample> {
    // generate random samples, but we also want to use
    // the same samples set on each run.
    let mut rng = SmallRng::seed_from_u64(1234);
    let range = Uniform::from(1u32..=3u32);
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
    let mut group = c.benchmark_group("statistic");

    let sizes = [5, 10, 50, 100, 200, 500, 1000];
    for &size in &sizes {
        group.bench_function(format!("samples-{}", size), |b| {
            let samples = generate_samples(size);

            b.iter_batched(
                || samples.clone(),
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
    config = Criterion::default().noise_threshold(0.05);
    targets = bench_statistic
);
criterion_main!(benches);
