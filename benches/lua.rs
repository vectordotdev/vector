use criterion::{criterion_group, Benchmark, Criterion};
use indexmap::IndexMap;
use transforms::lua::v2::LuaConfig;
use vector::{
    topology::config::{TransformConfig, TransformContext},
    transforms::{self, Transform},
    Event,
};

fn add_fields(c: &mut Criterion) {
    let num_events: usize = 100_000;

    let key = "the key";
    let value = "this is the value";

    let key_atom_native = key.into();
    let value_bytes_native = value.into();
    let key_atom_v1 = key.into();
    let value_bytes_v1 = value.into();
    let key_atom_v2 = key.into();
    let value_bytes_v2 = value.into();

    c.bench(
        "lua_add_fields",
        Benchmark::new("native", move |b| {
            b.iter_with_setup(
                || {
                    let mut map = IndexMap::new();
                    map.insert(key.into(), toml::value::Value::String(value.to_owned()));
                    transforms::add_fields::AddFields::new(map, true)
                },
                |mut transform| {
                    for _ in 0..num_events {
                        let event = Event::new_empty_log();
                        let event = transform.transform(event).unwrap();
                        assert_eq!(event.as_log()[&key_atom_native], value_bytes_native);
                    }
                },
            )
        })
        .with_function("v1", move |b| {
            b.iter_with_setup(
                || {
                    let source = format!("event['{}'] = '{}'", key, value);
                    transforms::lua::v1::Lua::new(&source, vec![]).unwrap()
                },
                |mut transform| {
                    for _ in 0..num_events {
                        let event = Event::new_empty_log();
                        let event = transform.transform(event).unwrap();
                        assert_eq!(event.as_log()[&key_atom_v1], value_bytes_v1);
                    }
                },
            )
        })
        .with_function("v2", move |b| {
            b.iter_with_setup(
                || {
                    let config = format!(
                        r#"
                        hooks.process = """
                            function (event, emit)
                                event['{}'] = '{}'
                            end
                        """
                        "#,
                        key, value
                    );
                    transforms::lua::v2::Lua::new(&toml::from_str::<LuaConfig>(&config).unwrap())
                        .unwrap()
                },
                |mut transform| {
                    for _ in 0..num_events {
                        let event = Event::new_empty_log();
                        let event = transform.transform(event).unwrap();
                        assert_eq!(event.as_log()[&key_atom_v2], value_bytes_v2);
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
                    .build(TransformContext::new_test(rt.executor()))
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
        .with_function("v1", move |b| {
            b.iter_with_setup(
                || {
                    let source = r#"
                      if event["the_field"] ~= "0" then
                        event = nil
                      end
                    "#;
                    transforms::lua::v1::Lua::new(&source, vec![]).unwrap()
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
        .with_function("v2", move |b| {
            b.iter_with_setup(
                || {
                    let config = r#"
                        hooks.proces = """
                            if event["the_field"] ~= "0" then
                              event = nil
                            end
                        """
                    "#;
                    transforms::lua::v2::Lua::new(&toml::from_str(config).unwrap()).unwrap()
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
