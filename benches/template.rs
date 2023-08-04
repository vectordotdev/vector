use std::convert::TryFrom;

use chrono::Utc;
use criterion::{criterion_group, BatchSize, Criterion};
use vector::{config::log_schema, event::Event, event::LogEvent};

fn bench_elasticsearch_index(c: &mut Criterion) {
    use vector::template::Template;

    let mut group = c.benchmark_group("template");

    group.bench_function("dynamic", |b| {
        let index = Template::try_from("index-%Y.%m.%d").unwrap();
        let mut event = Event::Log(LogEvent::from("hello world"));
        event.as_mut_log().insert(
            log_schema().timestamp_key_target_path().unwrap(),
            Utc::now(),
        );

        b.iter_batched(
            || event.clone(),
            |event| index.render(&event),
            BatchSize::SmallInput,
        )
    });

    group.bench_function("static", |b| {
        let index = Template::try_from("index").unwrap();
        let mut event = Event::Log(LogEvent::from("hello world"));
        event.as_mut_log().insert(
            log_schema().timestamp_key_target_path().unwrap(),
            Utc::now(),
        );

        b.iter_batched(
            || event.clone(),
            |event| index.render(&event),
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(
    name = benches;
    // encapsulates CI noise we saw in
    // https://github.com/vectordotdev/vector/issues/5394
    config = Criterion::default().noise_threshold(0.20);
    targets = bench_elasticsearch_index
);
