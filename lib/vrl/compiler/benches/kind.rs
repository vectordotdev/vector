use std::fmt;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use vrl_compiler::value::kind;

struct Parameters {
    basis: u16,
}

static PARAMETERS: [Parameters; 4] = [
    Parameters { basis: kind::BYTES },
    Parameters { basis: kind::ARRAY },
    Parameters { basis: kind::REGEX },
    Parameters { basis: kind::NULL },
];

impl fmt::Display for Parameters {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.basis)
    }
}

fn benchmark_kind_display(c: &mut Criterion) {
    let mut group = c.benchmark_group("vrl_compiler/value::kind::display");
    for param in &PARAMETERS {
        let parameter = vrl_compiler::Parameter {
            keyword: "",
            kind: param.basis,
            required: false,
        };

        let kind = parameter.kind();

        group.bench_with_input(BenchmarkId::from_parameter(param), &kind, |b, kind| {
            b.iter(|| kind.to_string())
        });
    }
}

criterion_group!(name = vrl_compiler_kind;
                 config = Criterion::default();
                 targets = benchmark_kind_display);
criterion_main!(vrl_compiler_kind);
