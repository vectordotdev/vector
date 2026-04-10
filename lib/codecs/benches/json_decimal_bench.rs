//! Benchmark for JSON deserialization with and without decimal precision preservation.
//!
//! Run with:
//!   cargo bench -p codecs --bench json_decimal_bench

use bytes::Bytes;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use vector_core::config::LogNamespace;

use codecs::decoding::format::{Deserializer, JsonDeserializer};

// Sample JSON payloads
const SIMPLE_JSON: &str = r#"{"message": "hello world", "level": "info", "count": 42}"#;

const NUMERIC_JSON: &str = r#"{
    "id": 12345,
    "temperature": 23.5,
    "pressure": 1013.25,
    "humidity": 65.0,
    "wind_speed": 12.3
}"#;

const HIGH_PRECISION_JSON: &str = r#"{
    "measurement": 12345678901234567890.12345678901234,
    "tolerance": 0.00000001234567890123,
    "total_count": 999999999999999999.99
}"#;

const MIXED_JSON: &str = r#"{
    "user_id": "abc123",
    "amount": 1234.56,
    "high_precision_amount": 12345678901234567890.12,
    "items": [1, 2, 3, 4, 5],
    "metadata": {"key": "value"}
}"#;

fn bench_json_deserialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_deserialize");

    // Test cases: (name, json_payload)
    let test_cases = [
        ("simple", SIMPLE_JSON),
        ("numeric", NUMERIC_JSON),
        ("high_precision", HIGH_PRECISION_JSON),
        ("mixed", MIXED_JSON),
    ];

    for (name, json) in test_cases {
        let input = Bytes::from(json);
        group.throughput(Throughput::Bytes(input.len() as u64));

        // Baseline: without decimal precision
        let deserializer =
            JsonDeserializer::new(false, codecs::decoding::format::ParseFloat::Float);
        group.bench_with_input(BenchmarkId::new("baseline", name), &input, |b, input| {
            b.iter(|| {
                deserializer
                    .parse(input.clone(), LogNamespace::Vector)
                    .expect("baseline deserialization should not fail")
            })
        });

        // With decimal precision enabled
        {
            let deserializer_precision =
                JsonDeserializer::new(false, codecs::decoding::format::ParseFloat::Decimal);
            group.bench_with_input(
                BenchmarkId::new("decimal_precision", name),
                &input,
                |b, input| {
                    b.iter(|| {
                        deserializer_precision
                            .parse(input.clone(), LogNamespace::Vector)
                            .expect("decimal precision deserialization should not fail")
                    })
                },
            );
        }
    }

    group.finish();
}

fn bench_json_deserialize_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_deserialize_batch");

    for size in [100, 1_000, 10_000, 100_000] {
        let batch: Vec<Bytes> = (0..size)
            .map(|i| {
                Bytes::from(format!(
                    r#"{{"id": {}, "value": 123.456, "big": 12345678901234567890.12}}"#,
                    i
                ))
            })
            .collect();

        let total_bytes: usize = batch.iter().map(|b| b.len()).sum();
        group.throughput(Throughput::Bytes(total_bytes as u64));

        let deserializer =
            JsonDeserializer::new(false, codecs::decoding::format::ParseFloat::Float);
        group.bench_function(format!("baseline_{size}"), |b| {
            b.iter(|| {
                for input in &batch {
                    let _ = deserializer
                        .parse(input.clone(), LogNamespace::Vector)
                        .expect("baseline batch deserialization should not fail");
                }
            })
        });

        let deserializer_precision =
            JsonDeserializer::new(false, codecs::decoding::format::ParseFloat::Decimal);
        group.bench_function(format!("decimal_precision_{size}"), |b| {
            b.iter(|| {
                for input in &batch {
                    let _ = deserializer_precision
                        .parse(input.clone(), LogNamespace::Vector)
                        .expect("decimal precision batch deserialization should not fail");
                }
            })
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_json_deserialize,
    bench_json_deserialize_batch
);
criterion_main!(benches);
