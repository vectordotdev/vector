use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use indexmap::IndexMap;
use lookup::lookup2::{JitPath, Path};
use lookup::{Look, Lookup};
use vector::event::util::log;
use vector::event::Event::Log;
use vector::event::{LogEvent, PathComponent, PathIter};
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
use vector_common::TimeZone;
use vrl::prelude::*;

criterion_group!(
    name = benches;
    // encapsulates CI noise we saw in
    // https://github.com/vectordotdev/vector/issues/5394
    config = Criterion::default().measurement_time(Duration::from_secs(60)).noise_threshold(0.05);
    targets = benchmark_lookup
);
criterion_main!(benches);

fn benchmark_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("lookup");

    // let c = |tform: &mut Box<dyn SyncTransform>, event: Event| {
    //     let mut outputs =
    //         TransformOutputsBuf::new_with_capacity(vec![Output::default(DataType::all())], 1);
    //     tform.transform(event, &mut outputs);
    //     let result = outputs.take_primary();
    //     let output_1 = result.first().unwrap().as_log();
    //
    //     debug_assert_eq!(output_1.get("foo").unwrap().to_string_lossy(), "bar");
    //     debug_assert_eq!(output_1.get("bar").unwrap().to_string_lossy(), "baz");
    //     debug_assert_eq!(output_1.get("copy").unwrap().to_string_lossy(), "buz");
    //
    //     result
    // };

    let mut value = value!({
        "foo": {
            "bar": {
                "asdf": [0, 1, 2, 3, 4, 5, 6, {"asdf": 42}, 8]
            }
        },
        "bar": "thing",
        "timestamp": 293658726
    });

    // let log_event = LogEvent::new

    let lookup_str = "foo.bar.asdf[7].asdf";
    let lookup = Lookup::from_str(lookup_str).unwrap();

    group.bench_function("lookup_clone", |b| {
        b.iter(|| {
            let lookup = lookup.clone();
        })
    });

    group.bench_function("lookup_get", |b| {
        b.iter(|| {
            let value = value.get(lookup.clone()).unwrap().unwrap();
            assert_eq!(*value, Value::Integer(42));
        })
    });

    group.bench_function("lookup_parse", |b| {
        b.iter(|| {
            let lookup = Lookup::from_str(lookup_str);
        })
    });

    group.bench_function("lookup_parse_and_get", |b| {
        b.iter(|| {
            let lookup = Lookup::from_str(lookup_str).unwrap();
            let value = value.get(lookup).unwrap().unwrap();
            assert_eq!(*value, Value::Integer(42));
        })
    });

    group.bench_function("path_iter_parse", |b| {
        b.iter(|| {
            let mut path_iter = PathIter::new(lookup_str);
            path_iter.count();
            // assert_eq!(path_iter.count(), 5);
        })
    });

    group.bench_function("lookup2_parse", |b| {
        b.iter(|| {
            JitPath::new(lookup_str).segment_iter().count();
            // let mut path_iter = PathIter::new(lookup_str);
            // path_iter.count();
            // assert_eq!(path_iter.count(), 5);
        })
    });

    group.bench_function("lookup2_parse_and_get", |b| {
        b.iter(|| {
            let jit_path = JitPath::new(lookup_str); //.iter();
            let value = log::get_value2(&value, jit_path).unwrap();
            assert_eq!(*value, Value::Integer(42));
            // let value = value.get2(&jit_path).unwrap().unwrap();
            // assert_eq!(*value, Value::Integer(42));
        })
    });

    group.bench_function("path_iter_pre-parsed_get", |b| {
        let mut path_iter = PathIter::new(lookup_str);
        let pre_parsed_vec: Vec<_> = path_iter.collect();
        let pre_parsed: [PathComponent; 5] = pre_parsed_vec.try_into().unwrap();

        b.iter(|| {
            let value = log::get_value(&value, pre_parsed.clone().into_iter()).unwrap();
            assert_eq!(*value, Value::Integer(42));
        })
    });

    group.bench_function("lookup2_pre-parsed_get", |b| {
        let pre_parsed_vec: Vec<_> = JitPath::new(lookup_str).segment_iter().collect();
        // let pre_parsed: [PathComponent; 5] = pre_parsed_vec.try_into().unwrap();

        b.iter(|| {
            let value = log::get_value2(&value, &pre_parsed_vec).unwrap();
            assert_eq!(*value, Value::Integer(42));
        })
    });

    group.bench_function("path_iter_parse_and_get", |b| {
        b.iter(|| {
            // let mut path_iter = PathIter::new(lookup_str);
            let value = log::get(value.as_object_mut_unwrap(), lookup_str).unwrap();
            assert_eq!(*value, Value::Integer(42));
        })
    });

    // group.bench_function("allocate_vec_size_5", |b| b.iter(|| vec![; 5]));
}
