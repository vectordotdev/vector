use std::collections::HashMap;

// use chrono::{DateTime, Utc};
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
// use indexmap::IndexMap;
use lookup::lookup_v2;
use lookup::{Look, Lookup};
// use vector::event::util::log;
// use vector::event::Event::Log;
// use vector::event::LogEvent;
// use vector::{
//     config::{DataType, Output},
//     event::{Event, Value},
// };
// use vrl::prelude::*;

criterion_group!(
    name = benches;
    // encapsulates CI noise we saw in
    // https://github.com/vectordotdev/vector/issues/5394
    config = Criterion::default().noise_threshold(0.05);
    targets = benchmark_lookup
);
criterion_main!(benches);

fn benchmark_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("lookup");

    // let mut value = value!({
    //     "foo": {
    //         "bar": {
    //             "asdf": [0, 1, 2, 3, 4, 5, 6, {"asdf": 42}, 8]
    //         }
    //     },
    //     "bar": "thing",
    //     "timestamp": 293658726
    // });

    let lookup_str = "foo.bar.asdf[7].asdf";
    let lookup_str_escaped = "foo.\"b.ar\".\"asdf\\\"asdf\".asdf[7].asdf";
    // let lookup = Lookup::from_str(lookup_str).unwrap();

    // group.bench_function("lookup_clone", |b| {
    //     b.iter(|| {
    //         let lookup = lookup.clone();
    //     })
    // });
    //
    // group.bench_function("lookup_get", |b| {
    //     b.iter(|| {
    //         let value = value.get(lookup.clone()).unwrap().unwrap();
    //         assert_eq!(*value, Value::Integer(42));
    //     })
    // });
    //
    // group.bench_function("lookup_parse", |b| {
    //     b.iter(|| {
    //         let lookup = Lookup::from_str(lookup_str);
    //     })
    // });

    group.bench_function("lookup_v2_parse", |b| {
        b.iter(|| {
            lookup_v2::Path::segment_iter(&lookup_str).count()
            // let lookup = Lookup::from_str(lookup_str);
        })
    });

    group.bench_function("lookup_v2_parse_escaped", |b| {
        b.iter(|| {
            lookup_v2::Path::segment_iter(&lookup_str_escaped).count()
            // let lookup = Lookup::from_str(lookup_str);
        })
    });

    // group.bench_function("lookup_parse_and_get", |b| {
    //     b.iter(|| {
    //         let lookup = Lookup::from_str(lookup_str).unwrap();
    //         let value = value.get(lookup).unwrap().unwrap();
    //         assert_eq!(*value, Value::Integer(42));
    //     })
    // });
}
