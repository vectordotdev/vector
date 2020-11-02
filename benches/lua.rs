use bytes::Bytes;
use criterion::{criterion_group, Benchmark, Criterion, Throughput};
use indexmap::IndexMap;
use transforms::lua::v2::LuaConfig;
use vector::{
    config::TransformConfig,
    test_util::runtime,
    transforms::{self, Transform},
    Event,
};

fn add_fields(c: &mut Criterion) {
    let key = "the key";
    let value = "this is the value";

    let value_bytes_native = Bytes::from(value).into();
    let value_bytes_v1 = Bytes::from(value).into();
    let value_bytes_v2 = Bytes::from(value).into();

    c.bench(
        "lua_add_fields",
        Benchmark::new("native", move |b| {
            b.iter_with_setup(
                || {
                    let mut map = IndexMap::new();
                    map.insert(String::from(key), String::from(value).into());
                    (
                        Event::new_empty_log(),
                        transforms::add_fields::AddFields::new(map, true).unwrap(),
                    )
                },
                |(event, mut transform)| {
                    let event = transform.transform(event).unwrap();
                    assert_eq!(event.as_log()[key], value_bytes_native);
                },
            )
        })
        .with_function("v1", move |b| {
            b.iter_with_setup(
                || {
                    let source = format!("event['{}'] = '{}'", key, value);
                    (
                        Event::new_empty_log(),
                        transforms::lua::v1::Lua::new(&source, vec![]).unwrap(),
                    )
                },
                |(event, mut transform)| {
                    let event = transform.transform(event).unwrap();
                    assert_eq!(event.as_log()[key], value_bytes_v1);
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
                                event.log['{}'] = '{}'

                                emit(event)
                            end
                        """
                        "#,
                        key, value
                    );
                    (
                        Event::new_empty_log(),
                        transforms::lua::v2::Lua::new(
                            &toml::from_str::<LuaConfig>(&config).unwrap(),
                        )
                        .unwrap(),
                    )
                },
                |(event, mut transform)| {
                    let event = transform.transform(event).unwrap();
                    assert_eq!(event.as_log()[key], value_bytes_v2);
                },
            )
        })
        .throughput(Throughput::Elements(1)),
    );
}

fn field_filter(c: &mut Criterion) {
    let num_events = 10;
    c.bench(
        "lua_field_filter",
        Benchmark::new("native", move |b| {
            let mut rt = runtime();
            b.iter_with_setup(
                || {
                    let events = (0..num_events).map(|i| {
                        let mut event = Event::new_empty_log();
                        event.as_mut_log().insert("the_field", (i % 10).to_string());
                        event
                    });
                    let transform = rt.block_on(async move {
                        transforms::field_filter::FieldFilterConfig {
                            field: "the_field".to_string(),
                            value: "0".to_string(),
                        }
                        .build()
                        .await
                        .unwrap()
                    });

                    (events, transform)
                },
                |(events, mut transform)| {
                    let num = events.filter_map(|r| transform.transform(r)).count();
                    assert_eq!(num, num_events / 10);
                },
            )
        })
        .with_function("v1", move |b| {
            b.iter_with_setup(
                || {
                    let events = (0..num_events).map(|i| {
                        let mut event = Event::new_empty_log();
                        event.as_mut_log().insert("the_field", (i % 10).to_string());
                        event
                    });

                    let source = r#"
                      if event["the_field"] ~= "0" then
                        event = nil
                      end
                    "#;
                    (
                        events,
                        transforms::lua::v1::Lua::new(&source, vec![]).unwrap(),
                    )
                },
                |(events, mut transform)| {
                    let num = events.filter_map(|r| transform.transform(r)).count();
                    assert_eq!(num, num_events / 10);
                },
            )
        })
        .with_function("v2", move |b| {
            b.iter_with_setup(
                || {
                    let events = (0..num_events).map(|i| {
                        let mut event = Event::new_empty_log();
                        event.as_mut_log().insert("the_field", (i % 10).to_string());
                        event
                    });
                    let config = r#"
                        hooks.process = """
                            function (event, emit)
                                if event.log["the_field"] ~= "0" then
                                  event = nil
                                end
                                emit(event)
                            end
                        """
                    "#;
                    (
                        events,
                        transforms::lua::v2::Lua::new(&toml::from_str(config).unwrap()).unwrap(),
                    )
                },
                |(events, mut transform)| {
                    let num = events.filter_map(|r| transform.transform(r)).count();
                    assert_eq!(num, num_events / 10);
                },
            )
        })
        .throughput(Throughput::Elements(num_events as u64)),
    );
}

criterion_group!(lua, add_fields, field_filter);
