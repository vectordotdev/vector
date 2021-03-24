use criterion::{criterion_group, BenchmarkId, Criterion, Throughput};
use std::fmt;
use vector::event::util::log::PathIter;

struct Parameters {
    basis: &'static str,
}

impl fmt::Display for Parameters {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.basis)
    }
}

static PARAMETERS: [Parameters; 7] = [
    Parameters {
        basis: "flying.squirrels.are.everywhere",
    },
    Parameters {
        basis: "flying.squirrel[137][0].tail",
    },
    Parameters {
        basis: "flying[0].squirrel[1]",
    },
    Parameters {
        basis: "flying\\[0\\]\\.squirrel[1].\\\\tail\\\\",
    },
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

fn benchmark_path_iter(c: &mut Criterion) {
    let mut group = c.benchmark_group("event::util::log::PathIter");
    for param in &PARAMETERS {
        group.throughput(Throughput::Bytes(param.basis.len() as u64));

        group.bench_with_input(BenchmarkId::from_parameter(param), &param, |b, &param| {
            let iter = PathIter::new(param.basis);
            b.iter(|| while let Some(_) = iter.next() {})
        });
    }
}

criterion_group!(name = benches;
                 config = Criterion::default();
                 targets = benchmark_path_iter);
