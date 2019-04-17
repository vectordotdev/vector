use criterion::{criterion_group, Benchmark, Criterion, Throughput};
use futures::sync::mpsc;
use futures::{Future, Sink, Stream};
use vector::sinks::util::{BatchSink, Buffer};
use vector::test_util::random_lines;

fn batching(
    bench_name: &'static str,
    gzip: bool,
    max_size: usize,
    num_records: usize,
    record_len: usize,
) -> Benchmark {
    Benchmark::new(bench_name, move |b| {
        b.iter_with_setup(
            move || {
                let input = random_lines(record_len)
                    .take(num_records)
                    .map(|s| s.into_bytes())
                    .collect::<Vec<_>>();
                futures::stream::iter_ok::<_, ()>(input.into_iter())
            },
            |input| {
                let (tx, _rx) = mpsc::unbounded();
                let batch_sink =
                    BatchSink::new(tx.sink_map_err(|_| ()), Buffer::new(gzip), max_size);

                input.forward(batch_sink).wait().unwrap()
            },
        )
    })
    .sample_size(10)
    .noise_threshold(0.05)
    .throughput(Throughput::Bytes((num_records * record_len) as u32))
}

fn benchmark_batching(c: &mut Criterion) {
    c.bench(
        "batch",
        batching(
            "no compression 10mb with 2mb batches",
            false,
            2_000_000,
            100_000,
            100,
        ),
    );
    c.bench(
        "batch",
        batching("gzip 10mb with 2mb batches", true, 2_000_000, 100_000, 100),
    );
}

criterion_group!(batch, benchmark_batching);
