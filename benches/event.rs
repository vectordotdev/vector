use bytes::Bytes;
use criterion::{criterion_group, BatchSize, Criterion};
use vector::event::LogEvent;
use vrl::event_path;

fn benchmark_event_iterate(c: &mut Criterion) {
    let mut group = c.benchmark_group("event/iterate");

    group.bench_function("single-level", |b| {
        b.iter_batched_ref(
            || {
                let mut log = LogEvent::default();
                log.insert(event_path!("key1"), Bytes::from("value1"));
                log.insert(event_path!("key2"), Bytes::from("value2"));
                log.insert(event_path!("key3"), Bytes::from("value3"));
                log
            },
            |e| e.all_event_fields().unwrap().count(),
            BatchSize::SmallInput,
        )
    });

    group.bench_function("nested-keys", |b| {
        b.iter_batched_ref(
            || {
                let mut log = LogEvent::default();
                log.insert(
                    event_path!("key1", "nested1", "nested2"),
                    Bytes::from("value1"),
                );
                log.insert(
                    event_path!("key1", "nested1", "nested3"),
                    Bytes::from("value4"),
                );
                log.insert(event_path!("key3"), Bytes::from("value3"));
                log
            },
            |e| e.all_event_fields().unwrap().count(),
            BatchSize::SmallInput,
        )
    });

    group.bench_function("array", |b| {
        b.iter_batched_ref(
            || {
                let mut log = LogEvent::default();
                log.insert(event_path!("key1", "nested1", 0), Bytes::from("value1"));
                log.insert(event_path!("key1", "nested1", 1), Bytes::from("value2"));
                log
            },
            |e| e.all_event_fields().unwrap().count(),
            BatchSize::SmallInput,
        )
    });
}

fn benchmark_event_create(c: &mut Criterion) {
    let mut group = c.benchmark_group("event/create");

    group.bench_function("single-level", |b| {
        b.iter(|| {
            let mut log = LogEvent::default();
            log.insert(event_path!("key1"), Bytes::from("value1"));
            log.insert(event_path!("key2"), Bytes::from("value2"));
            log.insert(event_path!("key3"), Bytes::from("value3"));
        })
    });

    group.bench_function("nested-keys", |b| {
        b.iter(|| {
            let mut log = LogEvent::default();
            log.insert(
                event_path!("key1", "nested1", "nested2"),
                Bytes::from("value1"),
            );
            log.insert(
                event_path!("key1", "nested1", "nested3"),
                Bytes::from("value4"),
            );
            log.insert(event_path!("key3"), Bytes::from("value3"));
        })
    });
    group.bench_function("array", |b| {
        b.iter(|| {
            let mut log = LogEvent::default();
            log.insert(event_path!("key1", "nested1", 0), Bytes::from("value1"));
            log.insert(event_path!("key1", "nested1", 1), Bytes::from("value2"));
        })
    });
}

criterion_group!(
    name = benches;
    // encapsulates inherent CI noise we saw in
    // https://github.com/vectordotdev/vector/issues/5394
    config = Criterion::default().noise_threshold(0.05);
    targets = benchmark_event_create, benchmark_event_iterate
);
