use bytes::Bytes;
use criterion::{criterion_group, BatchSize, Criterion};
use serde_json::{json, Value};
use vector::{
    config::log_schema,
    event::{Event, LogEvent},
    transforms::{
        json_parser::{JsonParser, JsonParserConfig},
        FunctionTransform, OutputBuffer,
    },
};

fn benchmark_event_iterate(c: &mut Criterion) {
    let mut group = c.benchmark_group("event/iterate");

    group.bench_function("single-level", |b| {
        b.iter_batched_ref(
            || {
                create_event(json!({
                    "key1": "value1",
                    "key2": "value2",
                    "key3": "value3"
                }))
            },
            |e| e.all_fields().count(),
            BatchSize::SmallInput,
        )
    });

    group.bench_function("nested-keys", |b| {
        b.iter_batched_ref(
            || {
                create_event(json!({
                    "key1": {
                        "nested1": {
                            "nested2": "value1",
                            "nested3": "value4"
                        }
                    },
                    "key3": "value3"
                }))
            },
            |e| e.all_fields().count(),
            BatchSize::SmallInput,
        )
    });

    group.bench_function("array", |b| {
        b.iter_batched_ref(
            || {
                create_event(json!({
                    "key1": {
                        "nested1": [
                            "value1",
                            "value2"
                        ]
                    },
                }))
            },
            |e| e.all_fields().count(),
            BatchSize::SmallInput,
        )
    });
}

fn benchmark_event_create(c: &mut Criterion) {
    let mut group = c.benchmark_group("event/create");

    group.bench_function("single-level", |b| {
        b.iter(|| {
            let mut log = Event::new_empty_log().into_log();
            log.insert("key1", Bytes::from("value1"));
            log.insert("key2", Bytes::from("value2"));
            log.insert("key3", Bytes::from("value3"));
        })
    });

    group.bench_function("nested-keys", |b| {
        b.iter(|| {
            let mut log = Event::new_empty_log().into_log();
            log.insert("key1.nested1.nested2", Bytes::from("value1"));
            log.insert("key1.nested1.nested3", Bytes::from("value4"));
            log.insert("key3", Bytes::from("value3"));
        })
    });
    group.bench_function("array", |b| {
        b.iter(|| {
            let mut log = Event::new_empty_log().into_log();
            log.insert("key1.nested1[0]", Bytes::from("value1"));
            log.insert("key1.nested1[1]", Bytes::from("value2"));
        })
    });
}

fn create_event(json: Value) -> LogEvent {
    let s = serde_json::to_string(&json).unwrap();
    let mut event = Event::new_empty_log();
    event.as_mut_log().insert(log_schema().message_key(), s);

    let mut parser = JsonParser::from(JsonParserConfig::default());
    let mut output = OutputBuffer::with_capacity(1);
    parser.transform(&mut output, event);
    output.into_events().next().unwrap().into_log()
}

criterion_group!(
    name = benches;
    // encapsulates inherent CI noise we saw in
    // https://github.com/timberio/vector/issues/5394
    config = Criterion::default().noise_threshold(0.05);
    targets = benchmark_event_create, benchmark_event_iterate
);
