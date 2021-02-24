use criterion::{criterion_group, BatchSize, BenchmarkId, Criterion, Throughput};
use indexmap::map::IndexMap;
use std::convert::TryFrom;
use std::{fs, io::Read, path::Path};
use vector::event::Lookup;

const FIXTURE_ROOT: &str = "tests/data/fixtures/lookup";

fn parse_artifact(path: impl AsRef<Path>) -> std::io::Result<String> {
    let mut test_file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) => return Err(e),
    };

    let mut buf = Vec::new();
    test_file.read_to_end(&mut buf)?;
    let string = String::from_utf8(buf).unwrap();
    Ok(string)
}

// This test iterates over the `tests/data/fixtures/lookup` folder and ensures the lookup parsed,
// then turned into a string again is the same.
fn lookup_to_string(c: &mut Criterion) {
    vector::test_util::trace_init();
    let mut fixtures = IndexMap::new();

    std::fs::read_dir(FIXTURE_ROOT)
        .unwrap()
        .for_each(|fixture_file| match fixture_file {
            Ok(fixture_file) => {
                let path = fixture_file.path();
                tracing::trace!(?path, "Opening.");
                let buf = parse_artifact(&path).unwrap();
                fixtures.insert(path, buf);
            }
            _ => panic!("This test should never read Err'ing test fixtures."),
        });

    let mut group_from_elem = c.benchmark_group("from_string");
    for (_path, fixture) in fixtures.iter() {
        group_from_elem.throughput(Throughput::Bytes(fixture.clone().into_bytes().len() as u64));
        group_from_elem.bench_with_input(
            BenchmarkId::from_parameter(&fixture),
            &fixture.clone(),
            move |b, ref param| {
                let input = &(*param).clone();
                b.iter_batched(
                    || input.clone(),
                    |input| Lookup::try_from(input).unwrap(),
                    BatchSize::SmallInput,
                )
            },
        );
    }
    group_from_elem.finish();

    let mut group_to_string = c.benchmark_group("to_string");
    // encapsulates CI noise we saw in
    // https://github.com/timberio/vector/issues/5394
    for (_path, fixture) in fixtures.iter() {
        group_to_string.throughput(Throughput::Bytes(fixture.clone().into_bytes().len() as u64));
        group_to_string.bench_with_input(
            BenchmarkId::from_parameter(&fixture),
            &fixture.clone(),
            move |b, ref param| {
                let input = &(*param).clone();
                b.iter_batched(
                    || Lookup::try_from(input.clone()).unwrap(),
                    |input| input.to_string(),
                    BatchSize::SmallInput,
                )
            },
        );
    }
    group_to_string.finish();

    let mut group_serialize = c.benchmark_group("serialize");
    for (_path, fixture) in fixtures.iter() {
        group_serialize.throughput(Throughput::Bytes(fixture.clone().into_bytes().len() as u64));
        group_serialize.bench_with_input(
            BenchmarkId::from_parameter(&fixture),
            &fixture.clone(),
            move |b, ref param| {
                let input = &(*param).clone();
                b.iter_batched(
                    || Lookup::try_from(input.clone()).unwrap(),
                    |input| serde_json::to_string(&input),
                    BatchSize::SmallInput,
                )
            },
        );
    }
    group_serialize.finish();

    let mut group_deserialize = c.benchmark_group("deserialize");
    // encapsulates CI noise we saw in
    // https://github.com/timberio/vector/issues/5394
    for (_path, fixture) in fixtures.iter() {
        group_deserialize.throughput(Throughput::Bytes(fixture.clone().into_bytes().len() as u64));
        group_deserialize.bench_with_input(
            BenchmarkId::from_parameter(&fixture),
            &fixture.clone(),
            move |b, ref param| {
                let input = &(*param).clone();
                b.iter_batched(
                    || serde_json::to_string(&Lookup::try_from(input.clone()).unwrap()).unwrap(),
                    |input| serde_json::from_str::<Lookup>(&input).unwrap(),
                    BatchSize::SmallInput,
                )
            },
        );
    }
    group_deserialize.finish();
}

criterion_group!(
    name = benches;
    // encapsulates CI noise we saw in
    // https://github.com/timberio/vector/issues/5394
    config = Criterion::default().noise_threshold(0.05);
    targets = lookup_to_string
);
