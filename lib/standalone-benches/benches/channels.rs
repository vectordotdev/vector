use std::{time::Duration, usize};

use criterion::{
    async_executor, black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput,
};
use futures03::{SinkExt, StreamExt};

criterion_group!(benches, benchmark);
criterion_main!(benches);

fn benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("channels/data_size");
    for &(batches, batch_size, time_mul) in &[
        (1_000, 1, 1),
        (1_000, 10, 1),
        (1_000, 100, 1),
        (100_000, 1, 1),
        (100_000, 10, 1),
        (100_000, 100, 1),
    ] {
        group.noise_threshold(0.01);
        group.measurement_time(Duration::from_secs(10 * time_mul));
        group.warm_up_time(Duration::from_secs(3 * time_mul));
        group.throughput(Throughput::Elements(batches as u64 * batch_size as u64));
        group.bench_with_input(
            BenchmarkId::new("tokio 0.2", format!("{}x{}", batches, batch_size)),
            &(batches, batch_size),
            |b, &(batches, batch_size)| {
                b.to_async(async_executor::FuturesExecutor).iter_with_setup(
                    || {
                        (
                            tokio02::sync::mpsc::channel(1),
                            make_data(batches, batch_size),
                        )
                    },
                    |((mut tx, mut rx), input)| async move {
                        for item in input {
                            tx.send(item).await.unwrap();
                            black_box(rx.recv().await.unwrap());
                        }
                    },
                );
            },
        );
        group.bench_with_input(
            BenchmarkId::new("tokio 1.1", format!("{}x{}", batches, batch_size)),
            &(batches, batch_size),
            |b, &(batches, batch_size)| {
                b.to_async(async_executor::FuturesExecutor).iter_with_setup(
                    || {
                        (
                            tokio11::sync::mpsc::channel(1),
                            make_data(batches, batch_size),
                        )
                    },
                    |((tx, mut rx), input)| async move {
                        for item in input {
                            tx.send(item).await.unwrap();
                            black_box(rx.recv().await.unwrap());
                        }
                    },
                );
            },
        );
        group.bench_with_input(
            BenchmarkId::new("futures 0.3", format!("{}x{}", batches, batch_size)),
            &(batches, batch_size),
            |b, &(batches, batch_size)| {
                b.iter_with_setup(
                    || {
                        (
                            futures03::channel::mpsc::channel(0),
                            make_data(batches, batch_size),
                        )
                    },
                    |((mut tx, mut rx), input)| async move {
                        for item in input {
                            tx.send(item).await.unwrap();
                            black_box(rx.next().await.unwrap());
                        }
                    },
                );
            },
        );
        group.bench_with_input(
            BenchmarkId::new("async-std 1.9", format!("{}x{}", batches, batch_size)),
            &(batches, batch_size),
            |b, &(batches, batch_size)| {
                b.iter_with_setup(
                    || {
                        (
                            async_std19::channel::bounded(1),
                            make_data(batches, batch_size),
                        )
                    },
                    |((tx, mut rx), input)| async move {
                        for item in input {
                            tx.send(item).await.unwrap();
                            black_box(rx.next().await.unwrap());
                        }
                    },
                );
            },
        );
    }
    group.finish();
}

fn make_data(batches: usize, batch_size: usize) -> Vec<Vec<usize>> {
    (0..batches)
        .map(|_| black_box(vec![0; batch_size]))
        .collect()
}
