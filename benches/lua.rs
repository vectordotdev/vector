use std::pin::Pin;

use criterion::{criterion_group, BatchSize, Criterion, Throughput};
use futures::{stream, SinkExt, Stream, StreamExt};
use indoc::indoc;
use transforms::lua::v2::LuaConfig;
use vector::{
    event::{Event, LogEvent},
    test_util::collect_ready,
    transforms::{self, OutputBuffer, Transform},
};
use vrl::event_path;

fn bench_add_fields(c: &mut Criterion) {
    let event = Event::from(LogEvent::default());

    let key = "the_key";
    let value = "this is the value";

    let mut group = c.benchmark_group("lua/add_fields");
    group.throughput(Throughput::Elements(1));

    let benchmarks: Vec<(&str, Transform)> = vec![
        ("v1", {
            let source = format!("event['{}'] = '{}'", key, value);

            Transform::event_task(transforms::lua::v1::Lua::new(source, vec![]).unwrap())
        }),
        ("v2", {
            let config = format!(
                indoc! {r#"
                    hooks.process = """
                    function (event, emit)
                      event.log['{}'] = '{}'

                      emit(event)
                    end
                    """
                "#},
                key, value
            );
            Transform::event_task(
                transforms::lua::v2::Lua::new(
                    &toml::from_str::<LuaConfig>(&config).unwrap(),
                    "transform".into(),
                )
                .unwrap(),
            )
        }),
    ];

    for (name, transform) in benchmarks {
        let (tx, rx) = futures::channel::mpsc::channel::<Event>(1);

        let mut rx: Pin<Box<dyn Stream<Item = Event> + Send>> = match transform {
            Transform::Function(t) => {
                let mut t = t.clone();
                Box::pin(rx.flat_map(move |v| {
                    let mut buf = OutputBuffer::with_capacity(1);
                    t.transform(&mut buf, v);
                    stream::iter(buf.into_events())
                }))
            }
            Transform::Synchronous(_t) => {
                unreachable!("no sync transform used in these benches");
            }
            Transform::Task(t) => t.transform_events(Box::pin(rx)),
        };

        group.bench_function(name.to_owned(), |b| {
            b.iter_batched(
                || (tx.clone(), event.clone()),
                |(mut tx, event)| {
                    futures::executor::block_on(tx.send(event)).unwrap();
                    let transformed = futures::executor::block_on(rx.next()).unwrap();

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
            let mut event = LogEvent::default();
            event.insert(event_path!("the_field"), (i % 10).to_string());
            Event::from(event)
        })
        .collect::<Vec<_>>();

    let mut group = c.benchmark_group("lua/field_filter");
    group.throughput(Throughput::Elements(num_events));

    let benchmarks: Vec<(&str, Transform)> = vec![
        ("v1", {
            let source = String::from(indoc! {r#"
                if event["the_field"] ~= "0" then
                    event = nil
                end
            "#});
            Transform::event_task(transforms::lua::v1::Lua::new(source, vec![]).unwrap())
        }),
        ("v2", {
            let config = indoc! {r#"
                hooks.process = """
                function (event, emit)
                  if event.log["the_field"] ~= "0" then
                    event = nil
                  end
                  emit(event)
                end
                """
            "#};
            Transform::event_task(
                transforms::lua::v2::Lua::new(&toml::from_str(config).unwrap(), "transform".into())
                    .unwrap(),
            )
        }),
    ];

    for (name, transform) in benchmarks {
        let (tx, rx) = futures::channel::mpsc::channel::<Event>(num_events as usize);

        let mut rx: Pin<Box<dyn Stream<Item = Event> + Send>> = match transform {
            Transform::Function(t) => {
                let mut t = t.clone();
                Box::pin(rx.flat_map(move |v| {
                    let mut buf = OutputBuffer::with_capacity(1);
                    t.transform(&mut buf, v);
                    stream::iter(buf.into_events())
                }))
            }
            Transform::Synchronous(_t) => {
                unreachable!("no sync transform used in these benches");
            }
            Transform::Task(t) => t.transform_events(Box::pin(rx)),
        };

        group.bench_function(name.to_owned(), |b| {
            b.iter_batched(
                || (tx.clone(), events.clone()),
                |(mut tx, events)| {
                    let _ =
                        futures::executor::block_on(tx.send_all(&mut stream::iter(events).map(Ok)))
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

criterion_group!(
    name = benches;
    // encapsulates CI noise we saw in
    // https://github.com/vectordotdev/vector/issues/5394
    config = Criterion::default().noise_threshold(0.05);
    targets = bench_add_fields, bench_field_filter
);
