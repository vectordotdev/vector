use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::fmt;
use std::str::FromStr;
use vrl_compiler::path::Path;

struct Parameters {
    basis: &'static str,
}

static PARAMETERS: [Parameters; 3] = [
    Parameters {
        basis: ".foo.(bar | baz)[1].(qux | quux)",
    },
    Parameters {
        basis: ".foo.(bar | baz)[1].(qux | quux).(a|b)",
    },
    Parameters {
        basis: ".foo.(bar | baz)[1].(qux | quux).(a|b).(c|d)",
    },
];

impl fmt::Display for Parameters {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.basis)
    }
}

fn benchmark_to_alternative_strings(c: &mut Criterion) {
    let mut group = c.benchmark_group("vrl_compiler/path::to_alternative_strings");
    for param in &PARAMETERS {
        group.throughput(Throughput::Bytes(param.basis.len() as u64));

        group.bench_with_input(BenchmarkId::from_parameter(param), &param, |b, &param| {
            let path = Path::from_str(param.basis).unwrap();
            b.iter(|| path.to_alternative_strings())
        });
    }
}

criterion_group!(name = vrl_compiler_path;
                 config = Criterion::default();
                 targets = benchmark_to_alternative_strings);
criterion_main!(vrl_compiler_path);
