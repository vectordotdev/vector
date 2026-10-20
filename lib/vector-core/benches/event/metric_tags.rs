use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    hint::black_box,
};

use criterion::{BatchSize, BenchmarkId, Criterion, SamplingMode, criterion_group};
use vector_core::event::{
    MetricTags,
    metric::{TagValue, TagValueSet},
};

fn tag_values(count: usize) -> Vec<TagValue> {
    (0..count)
        .map(|index| TagValue::Value(format!("value_{index:03}_us-east-1")))
        .collect()
}

fn tag_value_sets(c: &mut Criterion) {
    let mut group = c.benchmark_group("TagValueSet");
    group.sampling_mode(SamplingMode::Auto);

    for count in [1, 2, 8] {
        let values = tag_values(count);
        group.bench_with_input(BenchmarkId::new("build", count), &values, |b, values| {
            b.iter(|| black_box(values.iter().cloned().collect::<TagValueSet>()));
        });

        let set = values.iter().cloned().collect::<TagValueSet>();
        group.bench_with_input(BenchmarkId::new("as_single", count), &set, |b, set| {
            b.iter(|| black_box(set.as_single()));
        });
        group.bench_with_input(BenchmarkId::new("iterate", count), &set, |b, set| {
            b.iter(|| {
                let bytes = set
                    .iter()
                    .map(|value| value.map_or(0, str::len))
                    .sum::<usize>();
                black_box(bytes);
            });
        });
        group.bench_with_input(BenchmarkId::new("hash", count), &set, |b, set| {
            b.iter(|| {
                let mut hasher = DefaultHasher::new();
                set.hash(&mut hasher);
                black_box(hasher.finish());
            });
        });

        let equal = set.clone();
        group.bench_with_input(BenchmarkId::new("equal", count), &set, |b, set| {
            b.iter(|| black_box(set == black_box(&equal)));
        });
        group.bench_with_input(BenchmarkId::new("clone", count), &set, |b, set| {
            b.iter(|| black_box(set.clone()));
        });
        group.bench_function(BenchmarkId::new("consume_owned", count), |b| {
            b.iter_batched(
                || set.clone(),
                |set| black_box(set.into_iter().collect::<Vec<_>>()),
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn mixed_metric_tags() -> MetricTags {
    let mut tags = MetricTags::default();

    for index in 0..25 {
        tags.insert(
            format!("single_{index:03}"),
            format!("value_{index:03}_us-east-1"),
        );
    }
    for index in 0..4 {
        tags.insert(format!("bare_{index:03}"), TagValue::Bare);
    }
    for index in 0..3 {
        tags.set_multi_value(
            format!("multi_{index:03}"),
            (0..3).map(|value| TagValue::Value(format!("value_{index:03}_{value:03}"))),
        );
    }

    tags
}

fn metric_tags_mixed_32(c: &mut Criterion) {
    let mut group = c.benchmark_group("MetricTags/mixed_32");
    group.sampling_mode(SamplingMode::Auto);

    group.bench_function("build", |b| b.iter(|| black_box(mixed_metric_tags())));

    let tags = mixed_metric_tags();
    for (operation, key) in [
        ("lookup_single", "single_012"),
        ("lookup_bare", "bare_002"),
        ("lookup_multi", "multi_001"),
    ] {
        group.bench_with_input(BenchmarkId::new(operation, key), &tags, |b, tags| {
            b.iter(|| black_box(tags.get(black_box(key))));
        });
    }

    group.bench_with_input("iterate_all_values", &tags, |b, tags| {
        b.iter(|| {
            let bytes = tags
                .iter_all()
                .map(|(key, value)| key.len() + value.map_or(0, str::len))
                .sum::<usize>();
            black_box(bytes);
        });
    });

    for (operation, key) in [
        ("promote_single", "single_012"),
        ("insert_multi", "multi_001"),
    ] {
        group.bench_function(operation, |b| {
            b.iter_batched(
                || tags.clone(),
                |mut tags| {
                    tags.insert(black_box(key.to_owned()), black_box("value_new".to_owned()));
                    black_box(tags);
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.bench_with_input("hash", &tags, |b, tags| {
        b.iter(|| {
            let mut hasher = DefaultHasher::new();
            tags.hash(&mut hasher);
            black_box(hasher.finish());
        });
    });

    let equal = tags.clone();
    group.bench_with_input("equal", &tags, |b, tags| {
        b.iter(|| black_box(tags == black_box(&equal)));
    });
    group.bench_with_input("clone", &tags, |b, tags| {
        b.iter(|| black_box(tags.clone()));
    });
    group.bench_function("consume_owned", |b| {
        b.iter_batched(
            || tags.clone(),
            |tags| black_box(tags.into_iter_all().collect::<Vec<_>>()),
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

criterion_group!(benches, tag_value_sets, metric_tags_mixed_32);
