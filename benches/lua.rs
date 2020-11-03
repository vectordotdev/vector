use bytes::Bytes;
use criterion::{criterion_group, Benchmark, Criterion};
use futures::{compat::Stream01CompatExt, StreamExt};
use indexmap::IndexMap;
use transforms::lua::v2::LuaConfig;
use vector::{
    config::TransformConfig,
    test_util::runtime,
    transforms::{
        self, util::runtime_transform::RuntimeTransform, FunctionTransform, TaskTransform,
    },
    Event,
};

fn add_fields(c: &mut Criterion) {
    let num_events: usize = 100_000;

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
                    transforms::add_fields::AddFields::new(map, true).unwrap()
                },
                |transform| {
                    for _ in 0..num_events {
                        let event = Event::new_empty_log();
                        let mut output = vec![];
                        transform.transform(&mut output, event);
                        assert_eq!(output[0].as_log()[key], value_bytes_native);
                    }
                },
            )
        })
        .with_function("v1", move |b| {
            b.iter_with_setup(
                || {
                    let source = format!("event['{}'] = '{}'", key, value);
                    transforms::lua::v1::Lua::new(source, vec![]).unwrap()
                },
                |mut transform| {
                    for _ in 0..num_events {
                        let event = Event::new_empty_log();
                        let output = transform.transform_one(event).unwrap();
                        assert_eq!(output.as_log()[key], value_bytes_v1);
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
                        let mut output = Vec::with_capacity(1);
                        transform.transform(&mut output, event);
                        assert_eq!(output[0].as_log()[key], value_bytes_v2);
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
            let mut rt = runtime();
            b.iter_with_setup(
                || {
                    rt.block_on(async move {
                        transforms::field_filter::FieldFilterConfig {
                            field: "the_field".to_string(),
                            value: "0".to_string(),
                        }
                        .build()
                        .await
                        .unwrap()
                    })
                },
                |transform| {
                    let inputs = (0..num_events).map(|i| {
                        let mut event = Event::new_empty_log();
                        event.as_mut_log().insert("the_field", (i % 10).to_string());
                        event
                    });
                    let in_stream = futures01::stream::iter_ok(inputs);
                    let out_stream = transform
                        .into_task()
                        .transform(Box::new(in_stream))
                        .compat()
                        .collect::<Vec<_>>();
                    let blocked = futures::executor::block_on(out_stream);
                    let num = blocked.len();
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
                    transforms::lua::v1::Lua::new(source.to_string(), vec![]).unwrap()
                },
                |mut transform| {
                    let num = (0..num_events)
                        .map(|i| {
                            let mut event = Event::new_empty_log();
                            event.as_mut_log().insert("the_field", (i % 10).to_string());
                            event
                        })
                        .fold(Vec::new(), |mut acc, r| {
                            let item = transform.transform_one(r);
                            acc.push(item);
                            acc
                        })
                        .len();
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
                |transform| {
                    let inputs = (0..num_events).map(|i| {
                        let mut event = Event::new_empty_log();
                        event.as_mut_log().insert("the_field", (i % 10).to_string());
                        event
                    });
                    let in_stream = futures01::stream::iter_ok(inputs);
                    let out_stream =
                        TaskTransform::transform(Box::new(transform), Box::new(in_stream))
                            .compat()
                            .collect::<Vec<_>>();
                    let blocked = futures::executor::block_on(out_stream);
                    let num = blocked.len();
                    assert_eq!(num, num_events / 10);
                },
            )
        })
        .sample_size(10),
    );
}

criterion_group!(lua, add_fields, field_filter);
