use bytes::Bytes;
use criterion::{criterion_group, BatchSize, Criterion};
use serde_json::{json, Value};
use vector::{
    config::log_schema,
    event::{Event, LogEvent, LookupBuf},
    transforms::{
        json_parser::{JsonParser, JsonParserConfig},
        FunctionTransform,
    },
};

fn benchmark_event(c: &mut Criterion) {
    c.bench_function("create and insert single-level", |b| {
        b.iter(|| {
            let mut log = Event::new_empty_log().into_log();
            log.insert(LookupBuf::from("key1"), Bytes::from("value1"));
            log.insert(LookupBuf::from("key2"), Bytes::from("value2"));
            log.insert(LookupBuf::from("key3"), Bytes::from("value3"));
        })
    });

    c.bench_function("iterate all fields single-level", |b| {
        b.iter_batched_ref(
            || {
                create_event(json!({
                    "key1": "value1",
                    "key2": "value2",
                    "key3": "value3"
                }))
            },
            |e| e.pairs(true).count(),
            BatchSize::SmallInput,
        )
    });

    c.bench_function("create and insert nested-keys", |b| {
        b.iter(|| {
            let mut log = Event::new_empty_log().into_log();
            log.insert(
                LookupBuf::from_str("key1.nested1.nested2").unwrap(),
                Bytes::from("value1"),
            );
            log.insert(
                LookupBuf::from_str("key1.nested1.nested3").unwrap(),
                Bytes::from("value4"),
            );
            log.insert(LookupBuf::from_str("key3").unwrap(), Bytes::from("value3"));
        })
    });

    c.bench_function("iterate all fields nested-keys", |b| {
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
            |e| e.pairs(true).count(),
            BatchSize::SmallInput,
        )
    });

    c.bench_function("create and insert array", |b| {
        let str_1 = "key1.nested1[0]";
        let lookup_1 = LookupBuf::from(str_1);
        let str_2 = "key1.nested1[1]";
        let lookup_2 = LookupBuf::from(str_2);
        b.iter_batched(
            || (lookup_1.clone(), lookup_2.clone()),
            |(lookup_1, lookup_2)| {
                let mut log = Event::new_empty_log().into_log();
                log.insert(lookup_1, Bytes::from("value1"));
                log.insert(lookup_2, Bytes::from("value2"));
            },
            BatchSize::SmallInput,
        )
    });

    c.bench_function("iterate all fields array", |b| {
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
            |e| e.pairs(true).count(),
            BatchSize::SmallInput,
        )
    });
}

fn create_event(json: Value) -> LogEvent {
    let s = serde_json::to_string(&json).unwrap();
    let mut event = Event::new_empty_log();
    event
        .as_mut_log()
        .insert(log_schema().message_key().clone(), s);

    let mut parser = JsonParser::from(JsonParserConfig::default());
    let mut output = Vec::with_capacity(1);
    parser.transform(&mut output, event);
    output.into_iter().next().unwrap().into_log()
}

criterion_group!(benches, benchmark_event);
