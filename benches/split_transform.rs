use criterion::{black_box, criterion_group, Criterion};
use std::collections::HashMap;
use string_cache::DefaultAtom as Atom;

use vector::{
    transforms::{self, Transform},
    Event,
};

fn bench_split_transform(c: &mut Criterion) {
    let mut transform = Box::new(
        transforms::split::Split::new(
            vec!["value", "status", "addr"]
                .into_iter()
                .map(|s| s.into())
                .collect::<Vec<Atom>>(),
            Some(",".to_string()),
            "key".into(),
            false,
            HashMap::new(),
        )
        .unwrap(),
    );
    let mut input = Event::new_empty_log();
    input
        .as_mut_log()
        .insert("key".to_string(), "value,404,127.0.0.1".to_string());

    c.bench_function("split_transform", |bencher| {
        bencher.iter_with_setup(
            || input.clone(),
            |input| {
                let output = transform.transform(input);
                black_box(output)
            },
        )
    });
}

criterion_group!(split_transform, bench_split_transform);
