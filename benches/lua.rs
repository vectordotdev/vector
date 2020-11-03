use criterion::{black_box, criterion_group, BatchSize, Criterion, Throughput};
use indexmap::IndexMap;
use transforms::lua::v2::LuaConfig;
use vector::{
    config::TransformConfig,
    test_util::runtime,
    transforms::{self, Transform},
    Event,
};

fn bench_add_fields(c: &mut Criterion) {
    let event = Event::new_empty_log();

    let key = "the key";
    let value = "this is the value";

    let mut group = c.benchmark_group("lua_add_fields");
    group.throughput(Throughput::Elements(1));

    let mut benchmarks = [
        ("native", {
            let mut map = IndexMap::new();
            map.insert(String::from(key), value.to_owned().into());
            Box::new(transforms::add_fields::AddFields::new(map, true).unwrap())
                as Box<dyn Transform>
        }),
        ("v1", {
            let source = format!("event['{}'] = '{}'", key, value);

            Box::new(transforms::lua::v1::Lua::new(&source, vec![]).unwrap()) as Box<dyn Transform>
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
            Box::new(
                transforms::lua::v2::Lua::new(&toml::from_str::<LuaConfig>(&config).unwrap())
                    .unwrap(),
            ) as Box<dyn Transform>
        }),
    ];

    for (name, transform) in benchmarks.iter_mut() {
        group.bench_function(name.to_owned(), |b| {
            b.iter_batched(
                || event.clone(),
                |event| {
                    let event = black_box(transform.transform(event).unwrap());
                    debug_assert_eq!(event.as_log()[key], value.to_owned().into());
                },
                BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

fn bench_field_filter(c: &mut Criterion) {
    let num_events = 10;
    let events = (0..num_events).map(|i| {
        let mut event = Event::new_empty_log();
        event.as_mut_log().insert("the_field", (i % 10).to_string());
        event
    });

    let mut group = c.benchmark_group("lua_field_filter");
    group.throughput(Throughput::Elements(num_events));

    let mut benchmarks = [
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
            let source = r#"
    if event["the_field"] ~= "0" then
        event = nil
    end
    "#;
            Box::new(transforms::lua::v1::Lua::new(&source, vec![]).unwrap()) as Box<dyn Transform>
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
            Box::new(transforms::lua::v2::Lua::new(&toml::from_str(config).unwrap()).unwrap())
                as Box<dyn Transform>
        }),
    ];

    for (name, transform) in benchmarks.iter_mut() {
        group.bench_function(name.to_owned(), |b| {
            b.iter_batched(
                || events.clone(),
                |events| {
                    let num = black_box(events.filter_map(|r| transform.transform(r)).count());
                    debug_assert_eq!(num as u64, num_events / 10);
                },
                BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

criterion_group!(lua, bench_add_fields, bench_field_filter);
