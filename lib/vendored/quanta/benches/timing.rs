use criterion::{criterion_group, criterion_main, Bencher, Criterion};
use quanta::{Clock, Instant as QuantaInstant};
use std::time::Instant as StdInstant;

fn time_instant_now(b: &mut Bencher) {
    b.iter(StdInstant::now)
}

fn time_quanta_now(b: &mut Bencher) {
    let clock = Clock::new();
    b.iter(|| clock.now())
}

fn time_quanta_instant_now(b: &mut Bencher) {
    let _ = QuantaInstant::now();
    b.iter(QuantaInstant::now);
}

fn time_quanta_raw(b: &mut Bencher) {
    let clock = Clock::new();
    b.iter(|| clock.raw())
}

fn time_quanta_raw_scaled(b: &mut Bencher) {
    let clock = Clock::new();
    b.iter(|| clock.scaled(clock.raw()))
}

fn time_instant_delta(b: &mut Bencher) {
    b.iter(|| {
        let start = StdInstant::now();
        let d = StdInstant::now() - start;
        (d.as_secs() * 1_000_000_000) + u64::from(d.subsec_nanos())
    })
}

fn time_quanta_raw_delta(b: &mut Bencher) {
    let clock = Clock::new();
    b.iter(|| {
        let start = clock.raw();
        let end = clock.raw();
        clock.delta(start, end)
    })
}

fn time_quanta_raw_delta_as_nanos(b: &mut Bencher) {
    let clock = Clock::new();
    b.iter(|| {
        let start = clock.raw();
        let end = clock.raw();
        clock.delta_as_nanos(start, end)
    })
}

fn time_quanta_now_delta(b: &mut Bencher) {
    let clock = Clock::new();
    b.iter(|| {
        let start = clock.now();
        let end = clock.now();
        end - start
    })
}

fn time_quanta_recent(b: &mut Bencher) {
    let clock = Clock::new();
    b.iter(|| clock.recent())
}

fn time_quanta_instant_recent(b: &mut Bencher) {
    quanta::set_recent(QuantaInstant::now());
    b.iter(|| QuantaInstant::recent());
}

fn benchmark(c: &mut Criterion) {
    let mut std_group = c.benchmark_group("stdlib");
    std_group.bench_function("instant_now", time_instant_now);
    std_group.bench_function("instant_delta", time_instant_delta);
    std_group.finish();

    let mut q_group = c.benchmark_group("quanta");
    q_group.bench_function("quanta_now", time_quanta_now);
    q_group.bench_function("quanta_now_delta", time_quanta_now_delta);
    q_group.bench_function("quanta_instant_now", time_quanta_instant_now);
    q_group.bench_function("quanta_raw", time_quanta_raw);
    q_group.bench_function("quanta_raw_scaled", time_quanta_raw_scaled);
    q_group.bench_function("quanta_raw_delta", time_quanta_raw_delta);
    q_group.bench_function("quanta_raw_delta_as_nanos", time_quanta_raw_delta_as_nanos);
    q_group.bench_function("quanta_recent", time_quanta_recent);
    q_group.bench_function("quanta_instant_recent", time_quanta_instant_recent);
    q_group.finish();
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
