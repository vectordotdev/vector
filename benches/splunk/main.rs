use std::{io::Read, io::Write, time::Duration};

use bytes::{Buf, Bytes};
use criterion::{
    criterion_group, criterion_main, measurement::WallTime, BatchSize, BenchmarkGroup, Criterion,
    SamplingMode, Throughput,
};
use flate2::{read::MultiGzDecoder, write::GzEncoder, Compression};
use serde_json::{Deserializer, Value};

#[derive(Debug)]
struct Payload {
    body: Bytes,
    gzip: bool,
}

///
/// This benchmark examines the performance difference between two
/// implementations for reading a request body in the Splunk HEC source. The
/// goal of the "new" implementation was to replace invalid UTf8 bytes in the
/// body.
///
fn filter(c: &mut Criterion) {
    let mut group: BenchmarkGroup<WallTime> = c.benchmark_group("vector::sources::splunk_hec");
    group.sampling_mode(SamplingMode::Auto);

    let total_events = 1024; // arbitrary constant, the smaller the noisier
    group.throughput(Throughput::Elements(total_events as u64));
    group.bench_function("new", |b| {
        b.iter_batched(
            || Payload {
                body: Bytes::from(r#"{ "message": "hello world" }"#),
                gzip: false,
            },
            |payload| {
                let mut data = Vec::new();
                let body = if payload.gzip {
                    MultiGzDecoder::new(payload.body.reader())
                        .read_to_end(&mut data)
                        .unwrap();
                    String::from_utf8_lossy(data.as_slice())
                } else {
                    String::from_utf8_lossy(payload.body.as_ref())
                };

                Deserializer::from_str(&body).into_iter::<Value>();
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("new-gzip", |b| {
        b.iter_batched(
            || {
                let mut body = Vec::new();
                let mut e = GzEncoder::new(&mut body, Compression::default());
                e.write_all(b"{ \"message\": \"hello world\" }").unwrap();
                e.finish().unwrap();
                Payload {
                    body: Bytes::from(body),
                    gzip: true,
                }
            },
            |payload| {
                let mut data = Vec::new();
                let body = if payload.gzip {
                    MultiGzDecoder::new(payload.body.reader())
                        .read_to_end(&mut data)
                        .unwrap();
                    String::from_utf8_lossy(data.as_slice())
                } else {
                    String::from_utf8_lossy(payload.body.as_ref())
                };

                Deserializer::from_str(&body).into_iter::<Value>();
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("old", |b| {
        b.iter_batched(
            || Payload {
                body: Bytes::from(r#"{ "message": "hello world" }"#),
                gzip: false,
            },
            |payload| {
                let reader: Box<dyn Read + Send> = if payload.gzip {
                    Box::new(MultiGzDecoder::new(payload.body.reader()))
                } else {
                    Box::new(payload.body.reader())
                };

                Deserializer::from_reader(reader).into_iter::<Value>();
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("old-gzip", |b| {
        b.iter_batched(
            || {
                let mut body = Vec::new();
                let mut e = GzEncoder::new(&mut body, Compression::default());
                e.write_all(b"{ \"message\": \"hello world\" }").unwrap();
                e.finish().unwrap();
                Payload {
                    body: Bytes::from(body),
                    gzip: true,
                }
            },
            |payload| {
                let reader: Box<dyn Read + Send> = if payload.gzip {
                    Box::new(MultiGzDecoder::new(payload.body.reader()))
                } else {
                    Box::new(payload.body.reader())
                };

                Deserializer::from_reader(reader).into_iter::<Value>();
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(10))
        .measurement_time(Duration::from_secs(180))
        // degree of noise to ignore in measurements, here 1%
        .noise_threshold(0.01)
        // likelihood of noise registering as difference, here 5%
        .significance_level(0.05)
        // likelihood of capturing the true runtime, here 95%
        .confidence_level(0.95)
        // total number of bootstrap resamples, higher is less noisy but slower
        .nresamples(100_000)
        // total samples to collect within the set measurement time
        .sample_size(500);
    targets = filter
);

criterion_main!(benches);
