use criterion::{criterion_group, Criterion};
use serde_json::{json, Value};
use vector::{
    event::{self, Event, LogEvent},
    transforms::{
        json_parser::{JsonParser, JsonParserConfig},
        Transform,
    },
};

fn benchmark_event(c: &mut Criterion) {
    c.bench_function("create and insert single-level", |b| {
        b.iter(|| {
            let mut log = Event::new_empty_log().into_log();
            log.insert("key1", "value1");
            log.insert("key2", "value2");
            log.insert("key3", "value3");
        })
    });

    c.bench_function("iterate all fields single-level", |b| {
        b.iter_with_setup(
            || {
                create_event(json!({
                    "key1": "value1",
                    "key2": "value2",
                    "key3": "value3"
                }))
            },
            |e| e.all_fields().count(),
        )
    });

    c.bench_function("create and insert nested-keys", |b| {
        b.iter(|| {
            let mut log = Event::new_empty_log().into_log();
            log.insert("key1.nested1.nested2", "value1");
            log.insert("key1.nested1.nested3", "value4");
            log.insert("key3", "value3");
        })
    });

    c.bench_function("iterate all fields nested-keys", |b| {
        b.iter_with_setup(
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
        )
    });

    c.bench_function("create and insert array", |b| {
        b.iter(|| {
            let mut log = Event::new_empty_log().into_log();
            log.insert("key1.nested1[0]", "value1");
            log.insert("key1.nested1[1]", "value2");
        })
    });

    c.bench_function("iterate all fields array", |b| {
        b.iter_with_setup(
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
        )
    });
}

fn create_event(json: Value) -> LogEvent {
    let s = serde_json::to_string(&json).unwrap();
    let mut event = Event::new_empty_log();
    event
        .as_mut_log()
        .insert(event::log_schema().message_key().clone(), s);

    let mut parser = JsonParser::from(JsonParserConfig::default());
    parser.transform(event).unwrap().into_log()
}

criterion_group!(event, benchmark_event);
