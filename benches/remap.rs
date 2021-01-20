use chrono::{DateTime, Utc};
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use indexmap::IndexMap;
use remap::prelude::*;
use vector::transforms::{
    add_fields::AddFields,
    coercer::CoercerConfig,
    json_parser::{JsonParser, JsonParserConfig},
    remap::{Remap, RemapConfig},
    FunctionTransform,
};
use vector::{
    config::TransformConfig,
    event::{Event, Value},
    test_util::runtime,
};

criterion_group!(
    name = benches;
    // encapsulates CI noise we saw in
    // https://github.com/timberio/vector/issues/5394
    config = Criterion::default().noise_threshold(0.02);
    targets = benchmark_remap, upcase, downcase, parse_json
);
criterion_main!(benches);

bench_function! {
    upcase => remap_functions::Upcase;

    literal_value {
        args: func_args![value: "foo"],
        want: Ok("FOO")
    }
}

bench_function! {
    downcase => remap_functions::Downcase;

    literal_value {
        args: func_args![value: "FOO"],
        want: Ok("foo")
    }
}

bench_function! {
    parse_json => remap_functions::ParseJson;

    literal_value {
        args: func_args![value: r#"{"key": "value"}"#],
        want: Ok(value!({"key": "value"})),
    }
}

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

        let mut tform: Box<dyn FunctionTransform> = Box::new(AddFields::new(fields, true).unwrap());

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
                source: ".bar = parse_json!(.foo)".to_owned(),
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
        let mut tform: Box<dyn FunctionTransform> = Box::new(JsonParser::from(JsonParserConfig {
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
                source: r#"
                .number = to_int!(.number)
                .bool = to_bool!(.bool)
                .timestamp = parse_timestamp!(.timestamp, format: "%d/%m/%Y:%H:%M:%S %z")
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
        let mut tform: Box<dyn FunctionTransform> = rt
            .block_on(async move {
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
            })
            .into_function();

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
