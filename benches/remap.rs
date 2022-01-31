use std::collections::HashMap;

use chrono::{DateTime, Utc};
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use indexmap::IndexMap;
use shared::TimeZone;
use vector::{
    config::{DataType, Output},
    event::{Event, Value},
    transforms::{
        add_fields::AddFields,
        coercer::Coercer,
        json_parser::{JsonParser, JsonParserConfig},
        remap::{Remap, RemapConfig},
        SyncTransform, TransformOutputsBuf,
    },
};
use vrl::prelude::*;

criterion_group!(
    name = benches;
    // encapsulates CI noise we saw in
    // https://github.com/timberio/vector/issues/5394
    config = Criterion::default().noise_threshold(0.02);
    targets = benchmark_remap
);
criterion_main!(benches);

fn benchmark_remap(c: &mut Criterion) {
    let mut group = c.benchmark_group("remap");

    let add_fields_runner = |tform: &mut Box<dyn SyncTransform>, event: Event| {
        let mut outputs =
            TransformOutputsBuf::new_with_capacity(vec![Output::default(DataType::Any)], 1);
        tform.transform(event, &mut outputs);
        let result = outputs.take_primary();
        let output_1 = result.first().unwrap().as_log();

        debug_assert_eq!(output_1.get("foo").unwrap().to_string_lossy(), "bar");
        debug_assert_eq!(output_1.get("bar").unwrap().to_string_lossy(), "baz");
        debug_assert_eq!(output_1.get("copy").unwrap().to_string_lossy(), "buz");

        result
    };

    group.bench_function("add_fields/remap", |b| {
        let mut tform: Box<dyn SyncTransform> = Box::new(
            Remap::new(
                RemapConfig {
                    source: Some(
                        indoc! {r#".foo = "bar"
                            .bar = "baz"
                            .copy = string!(.copy_from)
                        "#}
                        .to_string(),
                    ),
                    file: None,
                    timezone: TimeZone::default(),
                    drop_on_error: true,
                    drop_on_abort: true,
                    ..Default::default()
                },
                &Default::default(),
            )
            .unwrap(),
        );

        let event = {
            let mut event = Event::from("augment me");
            event.as_mut_log().insert("copy_from", "buz".to_owned());
            event
        };

        b.iter_batched(
            || event.clone(),
            |event| add_fields_runner(&mut tform, event),
            BatchSize::SmallInput,
        );
    });

    group.bench_function("add_fields/native", |b| {
        let mut fields = IndexMap::new();
        fields.insert("foo".into(), String::from("bar").into());
        fields.insert("bar".into(), String::from("baz").into());
        fields.insert("copy".into(), String::from("{{ copy_from }}").into());

        let mut tform: Box<dyn SyncTransform> = Box::new(AddFields::new(fields, true).unwrap());

        let event = {
            let mut event = Event::from("augment me");
            event.as_mut_log().insert("copy_from", "buz".to_owned());
            event
        };

        b.iter_batched(
            || event.clone(),
            |event| add_fields_runner(&mut tform, event),
            BatchSize::SmallInput,
        );
    });

    let json_parser_runner = |tform: &mut Box<dyn SyncTransform>, event: Event| {
        let mut outputs =
            TransformOutputsBuf::new_with_capacity(vec![Output::default(DataType::Any)], 1);
        tform.transform(event, &mut outputs);
        let result = outputs.take_primary();
        let output_1 = result.first().unwrap().as_log();

        debug_assert_eq!(
            output_1.get("foo").unwrap().to_string_lossy(),
            r#"{"key": "value"}"#
        );
        debug_assert_eq!(
            output_1.get("bar").unwrap().to_string_lossy(),
            r#"{"key":"value"}"#
        );

        result
    };

    group.bench_function("parse_json/remap", |b| {
        let mut tform: Box<dyn SyncTransform> = Box::new(
            Remap::new(
                RemapConfig {
                    source: Some(".bar = parse_json!(string!(.foo))".to_owned()),
                    file: None,
                    timezone: TimeZone::default(),
                    drop_on_error: true,
                    drop_on_abort: true,
                    ..Default::default()
                },
                &Default::default(),
            )
            .unwrap(),
        );

        let event = {
            let mut event = Event::from("parse me");
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

    group.bench_function("parse_json/native", |b| {
        let mut tform: Box<dyn SyncTransform> = Box::new(JsonParser::from(JsonParserConfig {
            field: Some("foo".to_string()),
            target_field: Some("bar".to_owned()),
            drop_field: false,
            drop_invalid: false,
            overwrite_target: None,
        }));

        let event = {
            let mut event = Event::from("parse me");
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
            let mut outputs =
                TransformOutputsBuf::new_with_capacity(vec![Output::default(DataType::Any)], 1);
            tform.transform(event, &mut outputs);
            let result = outputs.take_primary();
            let output_1 = result.first().unwrap().as_log();

            debug_assert_eq!(output_1.get("number").unwrap(), &Value::Integer(1234));
            debug_assert_eq!(output_1.get("bool").unwrap(), &Value::Boolean(true));
            debug_assert_eq!(
                output_1.get("timestamp").unwrap(),
                &Value::Timestamp(timestamp),
            );

            result
        };

    group.bench_function("coerce/remap", |b| {
        let mut tform: Box<dyn SyncTransform> = Box::new(
            Remap::new(RemapConfig {
                source: Some(indoc! {r#"
                    .number = to_int!(.number)
                    .bool = to_bool!(.bool)
                    .timestamp = parse_timestamp!(string!(.timestamp), format: "%d/%m/%Y:%H:%M:%S %z")
                "#}
                .to_owned()),
                file: None,
                timezone: TimeZone::default(),
                drop_on_error: true,
                drop_on_abort: true,
                    ..Default::default()
            }, &Default::default())
            .unwrap(),
        );

        let mut event = Event::from("coerce me");
        for &(key, value) in &[
            ("number", "1234"),
            ("bool", "yes"),
            ("timestamp", "19/06/2019:17:20:49 -0400"),
        ] {
            event.as_mut_log().insert(key, value.to_owned());
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

    group.bench_function("coerce/native", |b| {
        let mut map = HashMap::new();
        map.insert(String::from("number"), String::from("int"));
        map.insert(String::from("bool"), String::from("bool"));
        map.insert(
            String::from("timestamp"),
            String::from("timestamp|%d/%m/%Y:%H:%M:%S %z"),
        );
        let types = vector::types::parse_conversion_map(&map, TimeZone::Local).unwrap();
        let mut tform: Box<dyn SyncTransform> = Box::new(Coercer::new(types, false));

        let mut event = Event::from("coerce me");
        for &(key, value) in &[
            ("number", "1234"),
            ("bool", "yes"),
            ("timestamp", "19/06/2019:17:20:49 -0400"),
        ] {
            event.as_mut_log().insert(key, value.to_owned());
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
