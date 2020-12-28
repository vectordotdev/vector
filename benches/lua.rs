use criterion::{criterion_group, BatchSize, Criterion, Throughput};
use futures::{
    compat::{Future01CompatExt, Stream01CompatExt},
    Stream, StreamExt,
};
use futures01::{Sink, Stream as Stream01};
use indexmap::IndexMap;
use transforms::lua::v2::LuaConfig;
use vector::{
    config::TransformConfig,
    test_util::{collect_ready, runtime},
    transforms::{self, Transform},
    Event,
};

fn bench_add_fields(c: &mut Criterion) {
    let event = Event::new_empty_log();

    let key = "the key";
    let value = "this is the value";

    let mut group = c.benchmark_group("lua_add_fields");
    group.throughput(Throughput::Elements(1));

    let benchmarks: Vec<(&str, Transform)> = vec![
        ("native", {
            let mut map = IndexMap::new();
            map.insert(String::from(key), value.to_owned().into());
            Transform::function(transforms::add_fields::AddFields::new(map, true).unwrap())
        }),
        ("v1", {
            let source = format!("event['{}'] = '{}'", key, value);

            Transform::task(transforms::lua::v1::Lua::new(source, vec![]).unwrap())
        }),
        ("v2", {
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
            Transform::task(
                transforms::lua::v2::Lua::new(&toml::from_str::<LuaConfig>(&config).unwrap())
                    .unwrap(),
            )
        }),
    ];

    for (name, transform) in benchmarks {
        let (tx, rx) = futures01::sync::mpsc::channel::<Event>(1);

        let mut rx: Box<dyn Stream<Item = Result<Event, ()>> + Send + Unpin> = match transform {
            Transform::Function(t) => {
                let mut t = t.clone();
                Box::new(
                    rx.map(move |v| {
                        let mut buf = Vec::with_capacity(1);
                        t.transform(&mut buf, v);
                        futures01::stream::iter_ok(buf.into_iter())
                    })
                    .flatten()
                    .compat(),
                )
            }
            Transform::Task(t) => Box::new(t.transform(Box::new(rx)).compat()),
        };

        group.bench_function(name.to_owned(), |b| {
            b.iter_batched(
                || (tx.clone(), event.clone()),
                |(tx, event)| {
                    futures::executor::block_on(tx.send(event).compat()).unwrap();
                    let transformed = futures::executor::block_on(rx.next()).unwrap().unwrap();

                    debug_assert_eq!(transformed.as_log()[key], value.to_owned().into());

                    transformed
                },
                BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

fn bench_field_filter(c: &mut Criterion) {
    let num_events = 10;
    let events = (0..num_events)
        .map(|i| {
            let mut event = Event::new_empty_log();
            event.as_mut_log().insert("the_field", (i % 10).to_string());
            event
        })
        .collect::<Vec<_>>();

    let mut group = c.benchmark_group("lua_field_filter");
    group.throughput(Throughput::Elements(num_events));

    let benchmarks: Vec<(&str, Transform)> = vec![
        ("native", {
            let mut rt = runtime();
            rt.block_on(async move {
                transforms::field_filter::FieldFilterConfig {
                    field: "the_field".to_string(),
                    value: "0".to_string(),
                }
                .build()
                .await
                .unwrap()
            })
        }),
        ("v1", {
            let source = String::from(
                r#"
if event["the_field"] ~= "0" then
event = nil
end
"#,
            );
            Transform::task(transforms::lua::v1::Lua::new(source, vec![]).unwrap())
        }),
        ("v2", {
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
            Transform::task(
                transforms::lua::v2::Lua::new(&toml::from_str(config).unwrap()).unwrap(),
            )
        }),
    ];

    for (name, transform) in benchmarks {
        let (tx, rx) = futures01::sync::mpsc::channel::<Event>(num_events as usize);

        let mut rx: Box<dyn Stream<Item = Result<Event, ()>> + Send + Unpin> = match transform {
            Transform::Function(t) => {
                let mut t = t.clone();
                Box::new(
                    rx.map(move |v| {
                        let mut buf = Vec::with_capacity(1);
                        t.transform(&mut buf, v);
                        futures01::stream::iter_ok(buf.into_iter())
                    })
                    .flatten()
                    .compat(),
                )
            }
            Transform::Task(t) => Box::new(t.transform(Box::new(rx)).compat()),
        };

        group.bench_function(name.to_owned(), |b| {
            b.iter_batched(
                || (tx.clone(), events.clone()),
                |(tx, events)| {
                    let _ = futures::executor::block_on(
                        tx.send_all(futures01::stream::iter_ok(events)).compat(),
                    )
                    .unwrap();

                    let output = futures::executor::block_on(collect_ready(&mut rx));

                    let num = output.len();

                    debug_assert_eq!(num as u64, num_events / 10);

                    num
                },
                BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

criterion_group!(benches, bench_add_fields, bench_field_filter);
