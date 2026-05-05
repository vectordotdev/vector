//! Microbenchmark comparing read-cost of `fast_clock::recent_millis()`
//! against `Instant::now()` and `Utc::now()` patterns used elsewhere in
//! Vector for histogram-binning timestamps.
//!
//! Run: `cargo bench --bench fast_clock -p vector-common`

use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use chrono::Utc;
use criterion::{Criterion, criterion_group, criterion_main};
use vector_common::fast_clock;

fn bench_clocks(c: &mut Criterion) {
    fast_clock::init();
    // Give the updater thread a moment to populate the cached value before
    // we start measuring. Avoids the very first read returning 0.
    std::thread::sleep(Duration::from_millis(50));

    let mut group = c.benchmark_group("clocks");

    group.bench_function("fast_clock::recent_millis", |b| {
        b.iter(|| black_box(fast_clock::recent_millis()))
    });

    group.bench_function("fast_clock::recent_unix_millis", |b| {
        b.iter(|| black_box(fast_clock::recent_unix_millis()))
    });

    group.bench_function("Instant::now", |b| b.iter(|| black_box(Instant::now())));

    group.bench_function("Instant_elapsed_as_millis", |b| {
        let epoch = Instant::now();
        b.iter(|| black_box(u64::try_from(epoch.elapsed().as_millis()).unwrap_or(u64::MAX)))
    });

    group.bench_function("Utc::now_timestamp_millis", |b| {
        b.iter(|| black_box(Utc::now().timestamp_millis()))
    });

    group.finish();
}

criterion_group!(benches, bench_clocks);
criterion_main!(benches);
