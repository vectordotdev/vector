use criterion::{black_box, criterion_group, BenchmarkId, Criterion};

fn benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("metrics_snapshot");
    // https://github.com/timberio/vector/runs/1746002475
    group.noise_threshold(0.02);
    for &cardinality in [0, 1, 10, 100, 1000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::new("cardinality", cardinality),
            &cardinality,
            |b, &cardinality| {
                let controller = prepare_metrics(cardinality);
                b.iter(|| {
                    let iter = vector::metrics::capture_metrics(controller);
                    assert_cardinality_matches(&iter, cardinality);
                    iter
                });
            },
        );
    }
    group.finish();
}

fn prepare_metrics(cardinality: usize) -> &'static vector::metrics::Controller {
    let _ = vector::metrics::init();
    let controller = vector::metrics::get_controller().unwrap();
    vector::metrics::reset(controller);

    for idx in 0..cardinality {
        metrics::counter!("test", 1, "idx" => format!("{}", idx));
    }

    assert_cardinality_matches(&vector::metrics::capture_metrics(controller), cardinality);

    controller
}

/// This call has negligible (and cosistent) performance compared to the rest
/// of the benches, however it performs the assertion over the data, effectively
/// acting as an implicit blackbox.
fn assert_cardinality_matches(iter: &impl Iterator, cardinality: usize) {
    let iter = black_box(iter);
    assert_eq!(iter.size_hint().0, cardinality);
    assert_eq!(iter.size_hint().1.unwrap(), cardinality);
}

criterion_group!(benches, benchmark);
