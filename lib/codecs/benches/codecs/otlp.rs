use bytes::{Bytes, BytesMut};
use chrono::{TimeZone, Utc};
use codecs::{
    decoding::{OtlpDeserializerConfig, format::Deserializer},
    encoding::OtlpSerializerConfig,
};
use criterion::{
    BatchSize, BenchmarkGroup, BenchmarkId, Criterion, Throughput, criterion_group,
    measurement::WallTime,
};
use opentelemetry_proto::proto::{
    collector::logs::v1::ExportLogsServiceRequest,
    common::v1::{AnyValue, InstrumentationScope, KeyValue, any_value::Value as PBValue},
    logs::v1::{LogRecord, ResourceLogs, ScopeLogs},
    resource::v1::Resource,
};
use prost::Message;
use tokio_util::codec::Encoder;
use vector_common::byte_size_of::ByteSizeOf;
use vector_core::{
    config::LogNamespace,
    event::{
        Event, Metric, MetricKind, MetricTags, MetricValue,
        metric::{Bucket, Quantile},
    },
    metric_tags,
};

fn timestamp() -> chrono::DateTime<Utc> {
    Utc.timestamp_nanos(1_700_000_000_000_000_000)
}

fn counter() -> Event {
    Metric::new(
        "requests_total",
        MetricKind::Incremental,
        MetricValue::Counter { value: 42.0 },
    )
    .with_tags(Some(metric_tags!(
        "host" => "web-01",
        "region" => "us-east-1",
        "resource.service.name" => "vector",
        "scope.name" => "otlp",
    )))
    .with_timestamp(Some(timestamp()))
    .into()
}

fn gauge() -> Event {
    Metric::new(
        "cpu_usage",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 12.5 },
    )
    .with_tags(Some(metric_tags!(
        "host" => "web-01",
        "core" => "0",
    )))
    .with_timestamp(Some(timestamp()))
    .into()
}

fn aggregated_histogram() -> Event {
    Metric::new(
        "request_latency_seconds",
        MetricKind::Absolute,
        MetricValue::AggregatedHistogram {
            buckets: vec![
                Bucket {
                    upper_limit: 0.005,
                    count: 10,
                },
                Bucket {
                    upper_limit: 0.01,
                    count: 20,
                },
                Bucket {
                    upper_limit: 0.025,
                    count: 30,
                },
                Bucket {
                    upper_limit: 0.05,
                    count: 25,
                },
                Bucket {
                    upper_limit: f64::INFINITY,
                    count: 15,
                },
            ],
            count: 100,
            sum: 1.75,
        },
    )
    .with_tags(Some(metric_tags!(
        "host" => "web-01",
        "route" => "/api/v1/query",
    )))
    .with_timestamp(Some(timestamp()))
    .into()
}

fn aggregated_summary() -> Event {
    Metric::new(
        "response_time_seconds",
        MetricKind::Absolute,
        MetricValue::AggregatedSummary {
            quantiles: vec![
                Quantile {
                    quantile: 0.5,
                    value: 0.01,
                },
                Quantile {
                    quantile: 0.9,
                    value: 0.05,
                },
                Quantile {
                    quantile: 0.99,
                    value: 0.1,
                },
            ],
            count: 1000,
            sum: 25.0,
        },
    )
    .with_tags(Some(metric_tags!(
        "host" => "web-01",
    )))
    .with_timestamp(Some(timestamp()))
    .into()
}

// Stresses tag splitting, which is where the bulk of the per-metric string allocations happen.
fn counter_many_tags() -> Event {
    let mut tags = MetricTags::default();
    for i in 0..32 {
        tags.insert(format!("tag_{i}"), format!("value_{i}"));
    }
    Metric::new(
        "events_total",
        MetricKind::Incremental,
        MetricValue::Counter { value: 1.0 },
    )
    .with_tags(Some(tags))
    .with_timestamp(Some(timestamp()))
    .into()
}

fn key_value(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue {
            value: Some(PBValue::StringValue(value.to_string())),
        }),
    }
}

fn log_record(body: &str, attributes: Vec<KeyValue>) -> LogRecord {
    LogRecord {
        time_unix_nano: 1_700_000_000_000_000_000,
        observed_time_unix_nano: 1_700_000_000_000_000_000,
        severity_number: 9, // INFO
        severity_text: "INFO".to_string(),
        body: Some(AnyValue {
            value: Some(PBValue::StringValue(body.to_string())),
        }),
        attributes,
        dropped_attributes_count: 0,
        flags: 0,
        trace_id: vec![],
        span_id: vec![],
    }
}

// The OTLP codec has no "native" log representation, so build a real OTLP logs request,
// then decode it once to obtain the OTLP-shaped `LogEvent` the encoder round-trips.
fn otlp_log_event(records: Vec<LogRecord>) -> Event {
    let request = ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(Resource {
                attributes: vec![key_value("service.name", "vector")],
                dropped_attributes_count: 0,
            }),
            scope_logs: vec![ScopeLogs {
                scope: Some(InstrumentationScope {
                    name: "otlp".to_string(),
                    version: "1.0".to_string(),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                log_records: records,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    };

    OtlpDeserializerConfig::default()
        .build()
        .parse(Bytes::from(request.encode_to_vec()), LogNamespace::Legacy)
        .expect("log decode should succeed")
        .into_iter()
        .next()
        .expect("expected one log event")
}

fn encode_to_bytes(input: &Event) -> Bytes {
    let mut serializer = OtlpSerializerConfig::default().build().unwrap();
    let mut buffer = BytesMut::new();
    serializer
        .encode(input.clone(), &mut buffer)
        .expect("encode should succeed");
    buffer.freeze()
}

fn bench_encode(group: &mut BenchmarkGroup<WallTime>, name: &str, event: &Event) {
    group.throughput(Throughput::Bytes(event.size_of() as u64));
    group.bench_with_input(BenchmarkId::new("encode", name), event, |b, event| {
        let mut serializer = OtlpSerializerConfig::default().build().unwrap();
        // Reused across iterations; sized by the first encode during warm-up.
        let mut buffer = BytesMut::new();
        b.iter_batched(
            || event.clone(),
            |event| {
                buffer.truncate(0);
                serializer.encode(event, &mut buffer).unwrap();
                buffer.len()
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_decode(group: &mut BenchmarkGroup<WallTime>, name: &str, encoded: &Bytes) {
    group.throughput(Throughput::Bytes(encoded.len() as u64));
    group.bench_with_input(BenchmarkId::new("decode", name), encoded, |b, encoded| {
        let deserializer = OtlpDeserializerConfig::default().build();
        b.iter_batched(
            || encoded.clone(),
            |encoded| deserializer.parse(encoded, LogNamespace::Legacy).unwrap(),
            BatchSize::SmallInput,
        );
    });
}

fn otlp(c: &mut Criterion) {
    let mut group: BenchmarkGroup<WallTime> = c.benchmark_group("otlp");

    let metrics: [(&str, Event); 5] = [
        ("counter", counter()),
        ("gauge", gauge()),
        ("aggregated_histogram", aggregated_histogram()),
        ("aggregated_summary", aggregated_summary()),
        ("counter_many_tags", counter_many_tags()),
    ];
    let metric_encoded: Vec<(&str, Bytes)> = metrics
        .iter()
        .map(|(name, event)| (*name, encode_to_bytes(event)))
        .collect();

    for (name, event) in &metrics {
        bench_encode(&mut group, name, event);
    }
    for (name, encoded) in &metric_encoded {
        bench_decode(&mut group, name, encoded);
    }

    // The whole batch in a single case, to measure amortized throughput across metric shapes.
    let metrics_in: u64 = metrics.iter().map(|(_, e)| e.size_of() as u64).sum();
    let mut metric_encoded_all = BytesMut::new();
    for (_, encoded) in &metric_encoded {
        metric_encoded_all.extend_from_slice(encoded);
    }
    let metric_encoded_all = metric_encoded_all.freeze();

    group.throughput(Throughput::Bytes(metrics_in));
    group.bench_function(BenchmarkId::new("encode", "all"), |b| {
        let mut serializer = OtlpSerializerConfig::default().build().unwrap();
        // Reused across iterations; sized by the first encode during warm-up.
        let mut buffer = BytesMut::new();
        b.iter_batched(
            || {
                metrics
                    .iter()
                    .map(|(_, e)| e.clone())
                    .collect::<Vec<Event>>()
            },
            |events| {
                buffer.truncate(0);
                for event in events {
                    serializer.encode(event, &mut buffer).unwrap();
                }
                buffer.len()
            },
            BatchSize::SmallInput,
        );
    });

    bench_decode(&mut group, "all", &metric_encoded_all);

    let logs: [(&str, Event); 2] = [
        (
            "log",
            otlp_log_event(vec![log_record(
                "user login succeeded",
                vec![
                    key_value("http.method", "GET"),
                    key_value("http.target", "/login"),
                    key_value("http.status_code", "200"),
                ],
            )]),
        ),
        (
            "log_many_attrs",
            otlp_log_event(vec![log_record(
                "request processed",
                (0..16)
                    .map(|i| key_value(&format!("attr_{i}"), &format!("value_{i}")))
                    .collect(),
            )]),
        ),
    ];
    for (name, event) in &logs {
        bench_encode(&mut group, name, event);
        bench_decode(&mut group, name, &encode_to_bytes(event));
    }
}

criterion_group!(benches, otlp);
