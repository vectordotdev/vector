use criterion::{black_box, criterion_group, Benchmark, Criterion};
use serde_json::Value;
use std::{collections::HashMap, fs, io::Read, path::Path};
use vector::{
    transforms::{wasm::Wasm, Transform},
    Event,
};

fn parse_config(s: &str) -> vector::Result<Wasm> {
    Wasm::new(toml::from_str(s).unwrap())
}

fn parse_event_artifact(path: impl AsRef<Path>) -> vector::Result<Event> {
    let mut event = Event::new_empty_log();
    let mut test_file = fs::File::open(path)?;

    let mut buf = String::new();
    test_file.read_to_string(&mut buf)?;
    let test_json: HashMap<String, Value> = serde_json::from_str(&buf)?;

    for (key, value) in test_json {
        event.as_mut_log().insert(key, value.clone());
    }
    Ok(event)
}

pub fn protobuf(c: &mut Criterion) {
    c.bench(
        "protobuf",
        Benchmark::new("wasm", move |b| {
            let mut transform = parse_config(
                r#"
            module = "target/wasm32-wasi/release/protobuf.wasm"
            "#,
            )
            .unwrap();

            let input = parse_event_artifact("tests/data/wasm/protobuf/demo.json").unwrap();

            b.iter_with_setup(
                || input.clone(),
                |input| {
                    let output = transform.transform(input);
                    black_box(output)
                },
            )
        }),
    );
}

pub fn noop(c: &mut Criterion) {
    c.bench(
        "noop",
        Benchmark::new("wasm", move |b| {
            let mut transform = parse_config(
                r#"
            module = "target/wasm32-wasi/release/noop.wasm"
            "#,
            )
            .unwrap();

            let input = Event::new_empty_log();

            b.iter_with_setup(
                || input.clone(),
                |input| {
                    let output = transform.transform(input);
                    black_box(output)
                },
            )
        }),
    );
}

criterion_group!(wasm, protobuf, noop);
