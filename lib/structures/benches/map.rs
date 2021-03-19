use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::fmt;
use std::mem;
use structures::map::Map;

struct Parameters {
    loops: usize,
}

impl fmt::Display for Parameters {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.loops)
    }
}

static PARAMETERS: [Parameters; 3] = [
    Parameters { loops: 8 },
    Parameters { loops: 128 },
    Parameters { loops: 256 },
];

fn benchmark_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert");
    for param in &PARAMETERS {
        group.throughput(Throughput::Bytes(
            (param.loops * mem::size_of::<u64>() * 2) as u64,
        ));

        group.bench_with_input(BenchmarkId::from_parameter(param), &param, |b, &param| {
            let mut map: Map<u64, u64> = Map::new();
            b.iter(|| {
                for cur in 0..param.loops {
                    map.insert(cur as u64, cur as u64);
                }
            })
        });
    }
}

fn benchmark_get(c: &mut Criterion) {
    let mut group = c.benchmark_group("get");
    for param in &PARAMETERS {
        group.throughput(Throughput::Bytes(
            (param.loops * mem::size_of::<u64>() * 2) as u64,
        ));

        group.bench_with_input(BenchmarkId::from_parameter(param), &param, |b, &param| {
            let mut map: Map<u64, u64> = Map::new();
            for cur in 0..param.loops {
                map.insert(cur as u64, cur as u64);
            }

            let get_pt: u64 = (param.loops / 2) as u64;
            b.iter(|| {
                map.get(&get_pt);
            });
        });
    }
}

fn benchmark_contains_key(c: &mut Criterion) {
    let mut group = c.benchmark_group("contains_key");
    for param in &PARAMETERS {
        group.throughput(Throughput::Bytes(
            (param.loops * mem::size_of::<u64>() * 2) as u64,
        ));

        group.bench_with_input(BenchmarkId::from_parameter(param), &param, |b, &param| {
            let mut map: Map<u64, u64> = Map::new();
            for cur in 0..param.loops {
                map.insert(cur as u64, cur as u64);
            }

            let get_pt: u64 = (param.loops / 2) as u64;
            b.iter(|| {
                map.contains_key(&get_pt);
            });
        });
    }
}

fn benchmark_clone(c: &mut Criterion) {
    let mut group = c.benchmark_group("clone");
    for param in &PARAMETERS {
        group.throughput(Throughput::Bytes(
            (param.loops * mem::size_of::<u64>() * 2) as u64,
        ));

        group.bench_with_input(BenchmarkId::from_parameter(param), &param, |b, &param| {
            let mut map: Map<u64, u64> = Map::new();
            for cur in 0..param.loops {
                map.insert(cur as u64, cur as u64);
            }

            b.iter(|| {
                let _ = map.clone();
            });
        });
    }
}

criterion_group!(name = benches;
                 config = Criterion::default();
                 targets = benchmark_insert, benchmark_get, benchmark_contains_key, benchmark_clone);
criterion_main!(benches);
