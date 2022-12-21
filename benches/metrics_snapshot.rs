use criterion::{criterion_group, BenchmarkId, Criterion};

fn benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("metrics_snapshot");
    // https://github.com/vectordotdev/vector/runs/1746002475
    group.noise_threshold(0.02);
    for &cardinality in [0, 1, 10, 100, 1000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::new("cardinality", cardinality),
            &cardinality,
            |b, &cardinality| {
                let controller = prepare_metrics(cardinality);
                b.iter(|| controller.capture_metrics());
            },
        );
    }
    group.finish();
}

fn prepare_metrics(cardinality: usize) -> &'static vector::metrics::Controller {
    vector::metrics::init_test();
    let controller = vector::metrics::Controller::get().unwrap();
    controller.reset();

    for idx in 0..cardinality {
        metrics::counter!("test", 1, "idx" => idx.to_string());
    }

    controller
}

criterion_group!(benches, benchmark);
