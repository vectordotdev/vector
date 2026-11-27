use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use quanta::Instant as QuantaInstant;
use std::{
    sync::Mutex,
    thread,
    time::{Duration, Instant as StdInstant},
};

const MAX_THREAD_COUNT: u32 = 12;

fn run<I>(thread_count: u32, iter_count: u64, now: impl Fn() -> I + Sync) -> Duration {
    let max_duration = Mutex::new(Duration::new(0, 0));

    thread::scope(|s| {
        for _ in 0..thread_count {
            s.spawn(|| {
                let start = StdInstant::now();

                for _ in 0..iter_count {
                    black_box(now());
                }

                let duration = start.elapsed();
                let mut max_duration = max_duration.lock().unwrap();
                *max_duration = max_duration.max(duration);
            });
        }
    });

    let max = *max_duration.lock().unwrap();
    max
}

fn benchmark(c: &mut Criterion) {
    let mut std_group = c.benchmark_group("stdlib");
    for thread_count in 1..=MAX_THREAD_COUNT {
        std_group.bench_with_input(
            BenchmarkId::new("now", thread_count),
            &thread_count,
            |b, &thread_count| {
                b.iter_custom(|iter_count| run(thread_count, iter_count, StdInstant::now));
            },
        );
    }
    std_group.finish();

    let mut q_group = c.benchmark_group("quanta");
    for thread_count in 1..=MAX_THREAD_COUNT {
        q_group.bench_with_input(
            BenchmarkId::new("now", thread_count),
            &thread_count,
            |b, &thread_count| {
                b.iter_custom(|iter_count| run(thread_count, iter_count, QuantaInstant::now));
            },
        );
    }
    q_group.finish();
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
