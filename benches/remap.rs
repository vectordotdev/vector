use std::collections::HashMap;

use chrono::{DateTime, Utc};
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use vector::{
    config::{DataType, TransformOutput},
    event::{Event, LogEvent, Value},
    transforms::{
        remap::{Remap, RemapConfig},
        SyncTransform, TransformOutputsBuf,
    },
};
use vrl::event_path;
use vrl::prelude::*;

criterion_group!(
    name = benches;
    // encapsulates CI noise we saw in
    // https://github.com/vectordotdev/vector/issues/5394
    config = Criterion::default().noise_threshold(0.02);
    targets = benchmark_remap
);
criterion_main!(benches);

fn benchmark_remap(c: &mut Criterion) {
    let mut group = c.benchmark_group("remap");

    let add_fields_runner = |tform: &mut Box<dyn SyncTransform>, event: Event| {
        let mut outputs = TransformOutputsBuf::new_with_capacity(
            vec![TransformOutput::new(DataType::all(), HashMap::new())],
            1,
        );
        tform.transform(event, &mut outputs);
        let result = outputs.take_primary();
        let output_1 = result.first().unwrap().as_log();

        debug_assert_eq!(
            output_1.get(event_path!("foo")).unwrap().to_string_lossy(),
            "bar"
        );
        debug_assert_eq!(
            output_1.get(event_path!("bar")).unwrap().to_string_lossy(),
            "baz"
        );
        debug_assert_eq!(
            output_1.get(event_path!("copy")).unwrap().to_string_lossy(),
            "buz"
        );

        result
    };

    group.bench_function("add_fields/remap", |b| {
        let mut tform: Box<dyn SyncTransform> = Box::new(
            Remap::new_ast(
                RemapConfig {
                    source: Some(
                        indoc! {r#".foo = "bar"
                            .bar = "baz"
                            .copy = string!(.copy_from)
                        "#}
                        .to_string(),
                    ),
                    file: None,
                    timezone: None,
                    drop_on_error: true,
                    drop_on_abort: true,
                    ..Default::default()
                },
                &Default::default(),
            )
            .unwrap()
            .0,
        );

        let event = {
            let mut event = Event::Log(LogEvent::from("augment me"));
            event
                .as_mut_log()
                .insert(event_path!("copy_from"), "buz".to_owned());
            event
        };

        b.iter_batched(
            || event.clone(),
            |event| add_fields_runner(&mut tform, event),
            BatchSize::SmallInput,
        );
    });

    let json_parser_runner = |tform: &mut Box<dyn SyncTransform>, event: Event| {
        let mut outputs = TransformOutputsBuf::new_with_capacity(
            vec![TransformOutput::new(DataType::all(), HashMap::new())],
            1,
        );
        tform.transform(event, &mut outputs);
        let result = outputs.take_primary();
        let output_1 = result.first().unwrap().as_log();

        debug_assert_eq!(
            output_1.get(event_path!("foo")).unwrap().to_string_lossy(),
            r#"{"key": "value"}"#
        );
        debug_assert_eq!(
            output_1.get(event_path!("bar")).unwrap().to_string_lossy(),
            r#"{"key":"value"}"#
        );

        result
    };

    group.bench_function("parse_json/remap", |b| {
        let mut tform: Box<dyn SyncTransform> = Box::new(
            Remap::new_ast(
                RemapConfig {
                    source: Some(".bar = parse_json!(string!(.foo))".to_owned()),
                    file: None,
                    timezone: None,
                    drop_on_error: true,
                    drop_on_abort: true,
                    ..Default::default()
                },
                &Default::default(),
            )
            .unwrap()
            .0,
        );

        let event = {
            let mut event = Event::Log(LogEvent::from("parse me"));
            event
                .as_mut_log()
                .insert("foo", r#"{"key": "value"}"#.to_owned());
            event
        };

        b.iter_batched(
            || event.clone(),
            |event| json_parser_runner(&mut tform, event),
            BatchSize::SmallInput,
        );
    });

    let coerce_runner =
        |tform: &mut Box<dyn SyncTransform>, event: Event, timestamp: DateTime<Utc>| {
            let mut outputs = TransformOutputsBuf::new_with_capacity(
                vec![TransformOutput::new(DataType::all(), HashMap::new())],
                1,
            );
            tform.transform(event, &mut outputs);
            let result = outputs.take_primary();
            let output_1 = result.first().unwrap().as_log();

            debug_assert_eq!(
                output_1.get(event_path!("number")).unwrap(),
                &Value::Integer(1234)
            );
            debug_assert_eq!(
                output_1.get(event_path!("bool")).unwrap(),
                &Value::Boolean(true)
            );
            debug_assert_eq!(
                output_1.get(event_path!("timestamp")).unwrap(),
                &Value::Timestamp(timestamp),
            );

            result
        };

    group.bench_function("coerce/remap", |b| {
        let mut tform: Box<dyn SyncTransform> = Box::new(
            Remap::new_ast(RemapConfig {
                source: Some(indoc! {r#"
                    .number = to_int!(.number)
                    .bool = to_bool!(.bool)
                    .timestamp = parse_timestamp!(string!(.timestamp), format: "%d/%m/%Y:%H:%M:%S %z")
                "#}
                .to_owned()),
                file: None,
                timezone: None,
                drop_on_error: true,
                drop_on_abort: true,
                    ..Default::default()
            }, &Default::default())
            .unwrap()
            .0,
        );

        let mut event = Event::Log(LogEvent::from("coerce me"));
        for &(key, value) in &[
            ("number", "1234"),
            ("bool", "yes"),
            ("timestamp", "19/06/2019:17:20:49 -0400"),
        ] {
            event.as_mut_log().insert(event_path!(key), value.to_owned());
        }

        let timestamp =
            DateTime::parse_from_str("19/06/2019:17:20:49 -0400", "%d/%m/%Y:%H:%M:%S %z")
                .unwrap()
                .with_timezone(&Utc);

        b.iter_batched(
            || event.clone(),
            |event| coerce_runner(&mut tform, event, timestamp),
            BatchSize::SmallInput,
        );
    });
}
