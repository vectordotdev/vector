use std::fmt;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use vrl_compiler::value::Kind;

struct Parameters {
    basis: Kind,
}

static PARAMETERS: [Parameters; 4] = [
    Parameters { basis: Kind::Bytes },
    Parameters { basis: Kind::Array },
    Parameters { basis: Kind::Regex },
    Parameters { basis: Kind::Null },
];

impl fmt::Display for Parameters {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.basis)
    }
}

fn benchmark_kind_display(c: &mut Criterion) {
    let mut group = c.benchmark_group("vrl_compiler/value::kind::display");
    for param in &PARAMETERS {
        group.bench_with_input(BenchmarkId::from_parameter(param), &param, |b, &param| {
            b.iter(|| format!("{}", param.basis))
        });
    }
}

criterion_group!(name = vrl_compiler_kind;
                 config = Criterion::default();
                 targets = benchmark_kind_display);
criterion_main!(vrl_compiler_kind);
