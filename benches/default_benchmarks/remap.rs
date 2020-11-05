use criterion::{criterion_group, BatchSize, Criterion};

use chrono::{DateTime, Utc};
use indexmap::IndexMap;

use vector::transforms::{
    add_fields::AddFields,
    coercer::CoercerConfig,
    json_parser::{JsonParser, JsonParserConfig},
    remap::{Remap, RemapConfig},
    Transform,
};
use vector::{
    config::TransformConfig,
    event::{Event, Value},
    test_util::runtime,
};

fn benchmark_remap(c: &mut Criterion) {
    let mut rt = runtime();
    let add_fields_runner = |tform: &mut Box<dyn Transform>, event: Event| {
        let result = tform.transform(event).unwrap();

        debug_assert_eq!(result.as_log().get("foo").unwrap().to_string_lossy(), "bar");
        debug_assert_eq!(result.as_log().get("bar").unwrap().to_string_lossy(), "baz");
        debug_assert_eq!(
            result.as_log().get("copy").unwrap().to_string_lossy(),
            "buz"
        );

        result
    };

    c.bench_function("remap: add fields with remap", |b| {
        let mut tform: Box<dyn Transform> = Box::new(
            Remap::new(RemapConfig {
                source: r#".foo = "bar"
            .bar = "baz"
            .copy = .copy_from"#
                    .to_string(),
                drop_on_err: true,
            })
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

    c.bench_function("remap: add fields with add_fields", |b| {
        let mut fields = IndexMap::new();
        fields.insert("foo".into(), String::from("bar").into());
        fields.insert("bar".into(), String::from("baz").into());
        fields.insert("copy".into(), String::from("{{ copy_from }}").into());

        let mut tform: Box<dyn Transform> = Box::new(AddFields::new(fields, true).unwrap());

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

    let json_parser_runner = |tform: &mut Box<dyn Transform>, event: Event| {
        let result = tform.transform(event).unwrap();

        debug_assert_eq!(
            result.as_log().get("foo").unwrap().to_string_lossy(),
            r#"{"key": "value"}"#
        );
        debug_assert_eq!(
            result.as_log().get("bar").unwrap().to_string_lossy(),
            r#"{"key":"value"}"#
        );

        result
    };

    c.bench_function("remap: parse JSON with remap", |b| {
        let mut tform: Box<dyn Transform> = Box::new(
            Remap::new(RemapConfig {
                source: ".bar = parse_json(.foo)".to_owned(),
                drop_on_err: false,
            })
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

    c.bench_function("remap: parse JSON with json_parser", |b| {
        let mut tform: Box<dyn Transform> = Box::new(JsonParser::from(JsonParserConfig {
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

    let coerce_runner = |tform: &mut Box<dyn Transform>, event: Event, timestamp: DateTime<Utc>| {
        let result = tform.transform(event).unwrap();

        debug_assert_eq!(
            result.as_log().get("number").unwrap(),
            &Value::Integer(1234)
        );
        debug_assert_eq!(result.as_log().get("bool").unwrap(), &Value::Boolean(true));
        debug_assert_eq!(
            result.as_log().get("timestamp").unwrap(),
            &Value::Timestamp(timestamp),
        );

        result
    };

    c.bench_function("remap: coerce with remap", |b| {
        let mut tform: Box<dyn Transform> = Box::new(
            Remap::new(RemapConfig {
                source: r#".number = to_int(.number)
                .bool = to_bool(.bool)
                .timestamp = parse_timestamp(.timestamp, format = "%d/%m/%Y:%H:%M:%S %z")
                "#
                .to_owned(),
                drop_on_err: true,
            })
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

    c.bench_function("remap: coerce with coercer", |b| {
        let mut tform: Box<dyn Transform> = rt.block_on(async move {
            toml::from_str::<CoercerConfig>(
                r#"drop_unspecified = false

                   [types]
                   number = "int"
                   bool = "bool"
                   timestamp = "timestamp|%d/%m/%Y:%H:%M:%S %z"
                   "#,
            )
            .unwrap()
            .build()
            .await
            .unwrap()
        });

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

criterion_group!(benches, benchmark_remap);
