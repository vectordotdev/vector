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
    c.bench_function("unflatten baseline", |b| {
        b.iter_with_setup(
            || {
                let mut e = Event::new_empty_log().into_log();
                e.insert_implicit("key1", "value1");
                e.insert_implicit("key2", "value2");
                e.insert_implicit("key3", "value3");

                e
            },
            |e| e.unflatten(),
        )
    });

    c.bench_function("unflatten single-level", |b| {
        b.iter_with_setup(
            || {
                create_event(json!({
                    "key1": "value1",
                    "key2": "value2",
                    "key3": "value3"
                }))
            },
            |e| e.unflatten(),
        )
    });

    c.bench_function("unflatten nested-keys", |b| {
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
            |e| e.unflatten(),
        )
    });

    c.bench_function("unflatten array", |b| {
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
            |e| e.unflatten(),
        )
    });
}

fn create_event(json: Value) -> LogEvent {
    let s = serde_json::to_string(&json).unwrap();
    let mut event = Event::new_empty_log();
    event
        .as_mut_log()
        .insert_implicit(event::MESSAGE.clone(), s);

    let mut parser = JsonParser::from(JsonParserConfig::default());
    parser.transform(event).unwrap().into_log()
}

criterion_group!(event, benchmark_event);
