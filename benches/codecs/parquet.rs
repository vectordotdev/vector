#![cfg(feature = "codecs-parquet")]

use std::time::Duration;

use bytes::BytesMut;
use criterion::{
    BatchSize, BenchmarkGroup, Criterion, SamplingMode, Throughput, criterion_group,
    measurement::WallTime,
};
use tokio_util::codec::Encoder;
use vector::event::{Event, LogEvent};
use vector_lib::{
    btreemap,
    byte_size_of::ByteSizeOf,
    codecs::{
        JsonSerializerConfig, NewlineDelimitedEncoder,
        encoding::{
            Framer,
            format::{
                ParquetCompression, ParquetFieldType, ParquetSchemaField, ParquetSerializerConfig,
                SchemaMode,
            },
        },
    },
};

fn make_events(count: usize) -> Vec<Event> {
    (0..count)
        .map(|i| {
            Event::Log(LogEvent::from(btreemap! {
                "message" => format!("log message number {} with some realistic content for benchmarking", i),
                "host" => format!("host-{}", i % 10),
                "level" => if i % 3 == 0 { "error" } else if i % 3 == 1 { "warn" } else { "info" },
                "status_code" => (200 + (i % 5) * 100) as i64,
            }))
        })
        .collect()
}

fn make_parquet_serializer(
    compression: ParquetCompression,
) -> vector_lib::codecs::encoding::format::ParquetSerializer {
    let config = ParquetSerializerConfig {
        schema: vec![
            ParquetSchemaField {
                name: "message".into(),
                data_type: ParquetFieldType::Utf8,
            },
            ParquetSchemaField {
                name: "host".into(),
                data_type: ParquetFieldType::Utf8,
            },
            ParquetSchemaField {
                name: "level".into(),
                data_type: ParquetFieldType::Utf8,
            },
            ParquetSchemaField {
                name: "status_code".into(),
                data_type: ParquetFieldType::Int64,
            },
        ],
        compression,
        schema_mode: SchemaMode::Relaxed,
    };
    vector_lib::codecs::encoding::format::ParquetSerializer::new(config).unwrap()
}

fn parquet_encoder(c: &mut Criterion) {
    let mut group: BenchmarkGroup<WallTime> = c.benchmark_group("parquet_encoder");
    group.sampling_mode(SamplingMode::Auto);

    let batch_sizes = [10, 100, 1000];

    for batch_size in batch_sizes {
        let events = make_events(batch_size);
        let total_bytes: u64 = events.iter().map(|e| e.size_of() as u64).sum();

        // Parquet Snappy (default)
        group.throughput(Throughput::Elements(batch_size as u64));
        group.bench_function(format!("parquet_snappy_{}_events", batch_size), |b| {
            b.iter_batched(
                || {
                    (
                        make_parquet_serializer(ParquetCompression::Snappy),
                        events.clone(),
                    )
                },
                |(mut encoder, events)| {
                    let mut bytes = BytesMut::new();
                    encoder.encode(events, &mut bytes).unwrap();
                    bytes
                },
                BatchSize::SmallInput,
            )
        });

        // Parquet Zstd
        group.throughput(Throughput::Elements(batch_size as u64));
        group.bench_function(format!("parquet_zstd_{}_events", batch_size), |b| {
            b.iter_batched(
                || {
                    (
                        make_parquet_serializer(ParquetCompression::Zstd),
                        events.clone(),
                    )
                },
                |(mut encoder, events)| {
                    let mut bytes = BytesMut::new();
                    encoder.encode(events, &mut bytes).unwrap();
                    bytes
                },
                BatchSize::SmallInput,
            )
        });

        // Parquet None (uncompressed)
        group.throughput(Throughput::Elements(batch_size as u64));
        group.bench_function(format!("parquet_none_{}_events", batch_size), |b| {
            b.iter_batched(
                || {
                    (
                        make_parquet_serializer(ParquetCompression::None),
                        events.clone(),
                    )
                },
                |(mut encoder, events)| {
                    let mut bytes = BytesMut::new();
                    encoder.encode(events, &mut bytes).unwrap();
                    bytes
                },
                BatchSize::SmallInput,
            )
        });

        // NDJSON baseline for comparison
        group.throughput(Throughput::Elements(batch_size as u64));
        group.bench_function(format!("ndjson_baseline_{}_events", batch_size), |b| {
            b.iter_batched(
                || {
                    let encoder = vector::codecs::Encoder::<Framer>::new(
                        NewlineDelimitedEncoder::default().into(),
                        JsonSerializerConfig::default().build().into(),
                    );
                    (encoder, events.clone())
                },
                |(mut encoder, events)| {
                    let mut bytes = BytesMut::new();
                    for event in events {
                        encoder.encode(event, &mut bytes).unwrap();
                    }
                    bytes
                },
                BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(3))
        .measurement_time(Duration::from_secs(10))
        .sample_size(50);
    targets = parquet_encoder
);
