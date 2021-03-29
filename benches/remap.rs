use chrono::{DateTime, Utc};
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use indexmap::IndexMap;
use remap::prelude::*;
use shared::log_event;
use vector::transforms::{
    add_fields::AddFields,
    coercer::CoercerConfig,
    json_parser::{JsonParser, JsonParserConfig},
    remap::{Remap, RemapConfig},
    FunctionTransform,
};
use vector::{
    config::{log_schema, GlobalOptions, TransformConfig},
    event::{Event, LookupBuf, Value},
    test_util::runtime,
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
    let mut rt = runtime();
    let add_fields_runner = |tform: &mut Box<dyn FunctionTransform>, event: Event| {
        let mut result = Vec::with_capacity(1);
        tform.transform(&mut result, event);
        let output_1 = result[0].as_log();

        debug_assert_eq!(output_1.get("foo").unwrap().to_string_lossy(), "bar");
        debug_assert_eq!(output_1.get("bar").unwrap().to_string_lossy(), "baz");
        debug_assert_eq!(output_1.get("copy").unwrap().to_string_lossy(), "buz");

        result
    };

    c.bench_function("remap: add fields with remap", |b| {
        let mut tform: Box<dyn FunctionTransform> = Box::new(
            Remap::new(RemapConfig {
                source: indoc! {r#".foo = "bar"
                    .bar = "baz"
                    .copy = string!(.copy_from)
                "#}
                .to_string(),
                drop_on_error: true,
            })
            .unwrap(),
        );

        let event = {
            let event = log_event! {
                log_schema().message_key().clone() => "augment me",
                log_schema().timestamp_key().clone() => chrono::Utc::now(),
                "copy_from" => "buz".to_owned(),
            };
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

        let mut tform: Box<dyn FunctionTransform> = Box::new(AddFields::new(fields, true).unwrap());

        let event = log_event! {
            log_schema().message_key().clone() => "augment me",
            log_schema().timestamp_key().clone() => chrono::Utc::now(),
            "copy_from" => "buz".to_owned(),
        };

        b.iter_batched(
            || event.clone(),
            |event| add_fields_runner(&mut tform, event),
            BatchSize::SmallInput,
        );
    });

    let json_parser_runner = |tform: &mut Box<dyn FunctionTransform>, event: Event| {
        let mut result = Vec::with_capacity(1);
        tform.transform(&mut result, event);
        let output_1 = result[0].as_log();

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

    c.bench_function("remap: parse JSON with remap", |b| {
        let mut tform: Box<dyn FunctionTransform> = Box::new(
            Remap::new(RemapConfig {
                source: ".bar = parse_json!(string!(.foo))".to_owned(),
                drop_on_error: false,
            })
            .unwrap(),
        );

        let event = log_event! {
            log_schema().message_key().clone() => "parse me",
            log_schema().timestamp_key().clone() => chrono::Utc::now(),
            "foo" => r#"{"key": "value"}"#.to_owned(),
        };

        b.iter_batched(
            || event.clone(),
            |event| json_parser_runner(&mut tform, event),
            BatchSize::SmallInput,
        );
    });

    c.bench_function("remap: parse JSON with json_parser", |b| {
        let mut tform: Box<dyn FunctionTransform> = Box::new(JsonParser::from(JsonParserConfig {
            field: Some(LookupBuf::from("foo")),
            target_field: Some(LookupBuf::from("bar")),
            drop_field: false,
            drop_invalid: false,
            overwrite_target: None,
        }));

        let event = log_event! {
            log_schema().message_key().clone() => "parse me",
            log_schema().timestamp_key().clone() => chrono::Utc::now(),
            "foo" => r#"{"key": "value"}"#.to_owned(),
        };

        b.iter_batched(
            || event.clone(),
            |event| json_parser_runner(&mut tform, event),
            BatchSize::SmallInput,
        );
    });

    let coerce_runner =
        |tform: &mut Box<dyn FunctionTransform>, event: Event, timestamp: DateTime<Utc>| {
            let mut result = Vec::with_capacity(1);
            tform.transform(&mut result, event);
            let output_1 = result[0].as_log();

            debug_assert_eq!(output_1.get("number").unwrap(), &Value::Integer(1234));
            debug_assert_eq!(output_1.get("bool").unwrap(), &Value::Boolean(true));
            debug_assert_eq!(
                output_1.get("timestamp").unwrap(),
                &Value::Timestamp(timestamp),
            );

            result
        };

    c.bench_function("remap: coerce with remap", |b| {
        let mut tform: Box<dyn FunctionTransform> = Box::new(
            Remap::new(RemapConfig {
                source: indoc! {r#"
                    .number = to_int!(.number)
                    .bool = to_bool!(.bool)
                    .timestamp = parse_timestamp!(string!(.timestamp), format: "%d/%m/%Y:%H:%M:%S %z")
                "#}
                .to_owned(),
                drop_on_error: true,
            })
            .unwrap(),
        );

        let mut event = log_event! {
            log_schema().message_key().clone() => "coerce me",
            log_schema().timestamp_key().clone() => chrono::Utc::now(),
        };
        for (key, value) in [
            (LookupBuf::from("number"), "1234".to_string()),
            (LookupBuf::from("bool"), "yes".to_string()),
            (
                LookupBuf::from("timestamp"),
                "19/06/2019:17:20:49 -0400".to_string(),
            ),
        ]
        .iter()
        {
            event.as_mut_log().insert(key.clone(), value.clone());
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
        let mut tform: Box<dyn FunctionTransform> = rt
            .block_on(async move {
                toml::from_str::<CoercerConfig>(indoc! {r#"
                        drop_unspecified = false

                        [types]
                        number = "int"
                        bool = "bool"
                        timestamp = "timestamp|%d/%m/%Y:%H:%M:%S %z"
                   "#})
                .unwrap()
                .build(&GlobalOptions::default())
                .await
                .unwrap()
            })
            .into_function();

        let mut event = log_event! {
            log_schema().message_key().clone() => "coerce me",
            log_schema().timestamp_key().clone() => chrono::Utc::now(),
        };
        for (key, value) in [
            (LookupBuf::from("number"), "1234".to_string()),
            (LookupBuf::from("bool"), "yes".to_string()),
            (
                LookupBuf::from("timestamp"),
                "19/06/2019:17:20:49 -0400".to_string(),
            ),
        ]
        .iter()
        {
            event.as_mut_log().insert(key.clone(), value.clone());
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
