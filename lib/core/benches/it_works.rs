use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::fmt;

struct Parameters {
    nth: u8,
}

impl fmt::Display for Parameters {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.nth)
    }
}

static PARAMETERS: [Parameters; 3] = [
    Parameters { nth: 1 },
    Parameters { nth: 8 },
    Parameters { nth: 16 },
];

fn fibonacci(nth: u8) -> u64 {
    match nth {
        0 => 0,
        1 => 1,
        2 => 1,
        3 => 2,
        4 => 3,
        5 => 5,
        6 => 8,
        7 => 13,
        8 => 21,
        _ => fibonacci(nth - 1) + fibonacci(nth - 2),
    }
}

fn bench_fibonacci(c: &mut Criterion) {
    let mut group = c.benchmark_group("fibonacci");
    for param in &PARAMETERS {
        group.bench_with_input(BenchmarkId::from_parameter(param), &param, |b, &param| {
            b.iter(|| fibonacci(param.nth))
        });
    }
}

criterion_group!(name = benches;
                 config = Criterion::default();
                 targets = bench_fibonacci);
criterion_main!(benches);
