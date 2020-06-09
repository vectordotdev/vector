use criterion::{black_box, criterion_group, criterion_main, Benchmark, BenchmarkId, Criterion};
use serde_json::Value;
use std::{collections::HashMap, fs, io::Read, path::Path};
use vector::{
    transforms::{wasm::Wasm, Transform},
    Event,
};

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
    let input = parse_event_artifact("tests/data/wasm/protobuf/demo.json").unwrap();
    let cloned_input = input.clone();
    c.bench(
        "wasm/protobuf",
        Benchmark::new("wasm", move |b| {
            let input = cloned_input.clone();
            let mut transform = Wasm::new(
                toml::from_str(
                    r#"
                module = "target/wasm32-wasi/release/protobuf.wasm"
                artifact_cache = "target/artifacts/"
                "#,
                )
                .unwrap(),
            )
            .unwrap();
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

pub fn drop(criterion: &mut Criterion) {
    let transforms: Vec<(&str, Box<dyn Transform>)> = vec![
        (
            "lua",
            Box::new(
                vector::transforms::lua::v2::Lua::new(
                    &toml::from_str(
                        r#"
                hooks.process = """
                function (event, emit)
                end
                """
                "#,
                    )
                    .unwrap(),
                )
                .unwrap(),
            ),
        ),
        (
            "wasm",
            Box::new(
                Wasm::new(
                    toml::from_str(
                        r#"
              module = "target/wasm32-wasi/release/drop.wasm"
              artifact_cache = "target/artifacts/"
              "#,
                    )
                    .unwrap(),
                )
                .unwrap(),
            ),
        ),
    ];
    let parameters = vec![0, 2, 8, 16];

    bench_group_transforms_over_parameterized_event_sizes(
        criterion,
        "wasm/drop",
        transforms,
        parameters,
    );
}

pub fn add_fields(criterion: &mut Criterion) {
    let transforms: Vec<(&str, Box<dyn Transform>)> = vec![
        (
            "lua",
            Box::new(
                vector::transforms::lua::v2::Lua::new(
                    &toml::from_str(
                        r#"
                hooks.process = """
                function (event, emit)
                    event.log.test_key = "test_value"
                    event.log.test_key2 = "test_value2"
                    emit(event)
                end
                """
                "#,
                    )
                    .unwrap(),
                )
                .unwrap(),
            ),
        ),
        (
            "wasm",
            Box::new(
                Wasm::new(
                    toml::from_str(
                        r#"
              module = "target/wasm32-wasi/release/add_fields.wasm"
              artifact_cache = "target/artifacts/"
              "#,
                    )
                    .unwrap(),
                )
                .unwrap(),
            ),
        ),
        (
            "native",
            Box::new({
                let mut fields = indexmap::IndexMap::default();
                fields.insert("test_key".into(), "test_value".into());
                fields.insert("test_key2".into(), "test_value2".into());
                vector::transforms::add_fields::AddFields::new(fields, false)
            }),
        ),
    ];
    let parameters = vec![0, 2, 8, 16];

    bench_group_transforms_over_parameterized_event_sizes(
        criterion,
        "wasm/add_fields",
        transforms,
        parameters,
    );
}

fn bench_group_transforms_over_parameterized_event_sizes(
    criterion: &mut Criterion,
    group: &str,
    transforms: Vec<(&str, Box<dyn Transform>)>,
    parameters: Vec<usize>,
) {
    let mut group = criterion.benchmark_group(group);
    for (name, mut transform) in transforms {
        for &parameter in &parameters {
            let mut input = Event::new_empty_log();
            for key in 0..parameter {
                input
                    .as_mut_log()
                    .insert(format!("key-{}", key), format!("value-{}", key));
            }

            let id = BenchmarkId::new(name.clone(), parameter);

            group.bench_with_input(id, &input, |bencher, input| {
                bencher.iter_with_setup(
                    || input.clone(),
                    |input| {
                        let output = transform.transform(input);
                        black_box(output)
                    },
                )
            });
        }
    }
    group.finish();
}

criterion_group!(wasm, protobuf, drop, add_fields);
criterion_main!(wasm);
