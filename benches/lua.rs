use criterion::{criterion_group, Benchmark, Criterion};
use indexmap::IndexMap;
use vector::{
    topology::config::TransformConfig,
    transforms::{self, Transform},
    Event,
};

fn add_fields(c: &mut Criterion) {
    let num_events: usize = 100_000;

    let key = "the key";
    let value = "this is the value";

    let key_atom = key.into();
    let value_bytes = value.into();
    let key_atom2 = key.into();
    let value_bytes2 = value.into();

    c.bench(
        "lua_add_fields",
        Benchmark::new("native", move |b| {
            b.iter_with_setup(
                || {
                    let mut map = IndexMap::new();
                    map.insert(key.into(), toml::value::Value::String(value.to_owned()));
                    transforms::add_fields::AddFields::new(map)
                },
                |mut transform| {
                    for _ in 0..num_events {
                        let event = Event::new_empty_log();
                        let event = transform.transform(event).unwrap();
                        assert_eq!(event.as_log()[&key_atom], value_bytes);
                    }
                },
            )
        })
        .with_function("lua", move |b| {
            b.iter_with_setup(
                || {
                    let source = format!("event['{}'] = '{}'", key, value);
                    transforms::lua::Lua::new(&source, vec![]).unwrap()
                },
                |mut transform| {
                    for _ in 0..num_events {
                        let event = Event::new_empty_log();
                        let event = transform.transform(event).unwrap();
                        assert_eq!(event.as_log()[&key_atom2], value_bytes2);
                    }
                },
            )
        })
        .sample_size(10),
    );
}

fn field_filter(c: &mut Criterion) {
    let num_events: usize = 100_000;

    c.bench(
        "lua_field_filter",
        Benchmark::new("native", move |b| {
            b.iter_with_setup(
                || {
                    let rt = vector::runtime::Runtime::single_threaded().unwrap();
                    transforms::field_filter::FieldFilterConfig {
                        field: "the_field".to_string(),
                        value: "0".to_string(),
                    }
                    .build(rt.executor())
                    .unwrap()
                },
                |mut transform| {
                    let num = (0..num_events)
                        .map(|i| {
                            let mut event = Event::new_empty_log();
                            event.as_mut_log().insert("the_field", (i % 10).to_string());
                            event
                        })
                        .filter_map(|r| transform.transform(r))
                        .count();
                    assert_eq!(num, num_events / 10);
                },
            )
        })
        .with_function("lua", move |b| {
            b.iter_with_setup(
                || {
                    let source = r#"
                      if event["the_field"] ~= "0" then
                        event = nil
                      end
                    "#;
                    transforms::lua::Lua::new(&source, vec![]).unwrap()
                },
                |mut transform| {
                    let num = (0..num_events)
                        .map(|i| {
                            let mut event = Event::new_empty_log();
                            event.as_mut_log().insert("the_field", (i % 10).to_string());
                            event
                        })
                        .filter_map(|r| transform.transform(r))
                        .count();
                    assert_eq!(num, num_events / 10);
                },
            )
        })
        .sample_size(10),
    );
}

criterion_group!(lua, add_fields, field_filter);
