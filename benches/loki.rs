use std::convert::TryFrom;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use vector::{sinks::loki::valid_label_name, template::Template};

const VALID: [&str; 4] = ["name", " name ", "bee_bop", "a09b"];
const INVALID: [&str; 4] = ["0ab", "*", "", " "];

fn bench_valid_label_name(c: &mut Criterion) {
    let mut group = c.benchmark_group("loki");

    group.bench_function("valid_label_name", |b| {
        for template in VALID {
            b.iter_batched(
                || Template::try_from(template).unwrap(),
                |label| valid_label_name(&label),
                BatchSize::SmallInput,
            );
        }
        for template in INVALID {
            b.iter_batched(
                || Template::try_from(template).unwrap(),
                |label| valid_label_name(&label),
                BatchSize::SmallInput,
            );
        }
    });
}

criterion_group!(benches, bench_valid_label_name);
criterion_main!(benches);
