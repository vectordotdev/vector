use bytes::Bytes;
use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use vector::event::LogEvent;
use vector::sources::dnstap::parser::DnstapParser;

fn benchmark_query_parsing(c: &mut Criterion) {
    let mut event = LogEvent::default();
    let raw_dnstap_data = "ChVqYW1lcy1WaXJ0dWFsLU1hY2hpbmUSC0JJTkQgOS4xNi4zcnoIAxACGAEiEAAAAAAAAA\
    AAAAAAAAAAAAAqECABBQJwlAAAAAAAAAAAADAw8+0CODVA7+zq9wVNMU3WNlI2kwIAAAABAAAAAAABCWZhY2Vib29rMQNjb\
    20AAAEAAQAAKQIAAACAAAAMAAoACOxjCAG9zVgzWgUDY29tAHgB";
    let dnstap_data = base64::decode(raw_dnstap_data).unwrap();

    let mut group = c.benchmark_group("dnstap");
    group.throughput(Throughput::Bytes(dnstap_data.len() as u64));
    group.bench_function("dns_query_parsing", |b| {
        b.iter_batched(
            || dnstap_data.clone(),
            |dnstap_data| DnstapParser::parse(&mut event, Bytes::from(dnstap_data)).unwrap(),
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn benchmark_update_parsing(c: &mut Criterion) {
    let mut event = LogEvent::default();
    let raw_dnstap_data = "ChVqYW1lcy1WaXJ0dWFsLU1hY2hpbmUSC0JJTkQgOS4xNi4zcmsIDhABGAEiBH8AAA\
    EqBH8AAAEwrG44AEC+iu73BU14gfofUh1wi6gAAAEAAAAAAAAHZXhhbXBsZQNjb20AAAYAAWC+iu73BW0agDwvch1wi6gAA\
    AEAAAAAAAAHZXhhbXBsZQNjb20AAAYAAXgB";
    let dnstap_data = base64::decode(raw_dnstap_data).unwrap();

    let mut group = c.benchmark_group("dnstap");
    group.throughput(Throughput::Bytes(dnstap_data.len() as u64));
    group.bench_function("dns_update_parsing", |b| {
        b.iter_batched(
            || dnstap_data.clone(),
            |dnstap_data| DnstapParser::parse(&mut event, Bytes::from(dnstap_data)).unwrap(),
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(
    name = benches;
    // encapsulates inherent CI noise we saw in
    // https://github.com/vectordotdev/vector/issues/5394
    config = Criterion::default().noise_threshold(0.05);
    targets = benchmark_query_parsing,benchmark_update_parsing
);

criterion_main! {
    benches,
}
