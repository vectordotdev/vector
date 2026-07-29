use std::time::Duration;

use bytes::BytesMut;
use chrono::{TimeZone as _, Utc};
use criterion::{
    BatchSize, BenchmarkGroup, BenchmarkId, Criterion, SamplingMode, Throughput, criterion_group,
    measurement::WallTime,
};
use tokio::runtime::{Handle, Runtime};
use vector_buffers::{BufferType, encoding::Encodable};
use vector_common::byte_size_of::ByteSizeOf;
use vector_core::{
    event::{
        EstimatedJsonEncodedSizeOf, EventArray, EventMetadata, LogEvent, Metric, MetricKind,
        MetricValue, ObjectMap, TraceEvent, Value,
    },
    metric_tags,
};

use crate::common::{
    BenchmarkState, DataDir, Operation, disk_buffer, init_instrumentation, memory_buffer_by_bytes,
    memory_buffer_by_events,
};

const TOTAL_EVENTS: usize = 1_000;
const ARRAY_LENGTHS: [usize; 2] = [100, 1_000];
const DISK_CAPACITY_MULTIPLIER: u64 = 2;
const MIN_DISK_BUFFER_SIZE: u64 = 268_435_488;

#[derive(Clone, Copy)]
struct Fixture {
    event_type: &'static str,
    profile: &'static str,
    make_array: fn(usize) -> EventArray,
}

const FIXTURES: [Fixture; 6] = [
    Fixture {
        event_type: "log",
        profile: "minimal",
        make_array: minimal_logs,
    },
    Fixture {
        event_type: "log",
        profile: "representative",
        make_array: representative_logs,
    },
    Fixture {
        event_type: "metric",
        profile: "minimal",
        make_array: minimal_metrics,
    },
    Fixture {
        event_type: "metric",
        profile: "representative",
        make_array: representative_metrics,
    },
    Fixture {
        event_type: "trace",
        profile: "minimal",
        make_array: minimal_traces,
    },
    Fixture {
        event_type: "trace",
        profile: "representative",
        make_array: representative_traces,
    },
];

#[derive(Clone, Copy)]
enum BufferKind {
    Disk,
    MemoryEvents,
    MemoryBytes,
}

impl BufferKind {
    const ALL: [Self; 3] = [Self::Disk, Self::MemoryEvents, Self::MemoryBytes];

    const fn name(self) -> &'static str {
        match self {
            Self::Disk => "disk",
            Self::MemoryEvents => "memory-events",
            Self::MemoryBytes => "memory-bytes",
        }
    }

    fn create(self, max_events: usize, memory_bytes: usize, disk_bytes: u64) -> BufferType {
        match self {
            Self::Disk => disk_buffer(disk_bytes),
            Self::MemoryEvents => memory_buffer_by_events(max_events),
            Self::MemoryBytes => memory_buffer_by_bytes(memory_bytes),
        }
    }
}

fn fields(entries: impl IntoIterator<Item = (&'static str, Value)>) -> ObjectMap {
    entries
        .into_iter()
        .map(|(key, value)| (key.into(), value))
        .collect()
}

fn timestamp() -> chrono::DateTime<Utc> {
    Utc.timestamp_nanos(1_700_000_000_000_000_000)
}

fn minimal_log() -> LogEvent {
    LogEvent::from_map(
        fields([("message", "request completed".into())]),
        EventMetadata::default(),
    )
}

fn representative_log() -> LogEvent {
    LogEvent::from_map(
        fields([
            ("message", "completed checkout request".into()),
            ("level", "info".into()),
            ("service", "checkout".into()),
            ("host", "web-01".into()),
            ("request_id", "018e1725-628d-7d06-8d31-33de0a1d82ef".into()),
            ("timestamp", timestamp().into()),
            (
                "http",
                Value::Object(fields([
                    ("method", "POST".into()),
                    ("route", "/api/v1/checkout".into()),
                    ("status_code", 200_i64.into()),
                ])),
            ),
            (
                "user",
                Value::Object(fields([
                    ("id", "usr_01HZX7D62ME2QJ4S9P5W8Y3K0F".into()),
                    ("tier", "premium".into()),
                    ("country", "US".into()),
                    ("authenticated", true.into()),
                    (
                        "scopes",
                        Value::Array(vec!["checkout:write".into(), "payment:read".into()]),
                    ),
                ])),
            ),
        ]),
        EventMetadata::default(),
    )
}

fn minimal_metric() -> Metric {
    Metric::new(
        "requests_total",
        MetricKind::Absolute,
        MetricValue::Counter { value: 1.0 },
    )
}

fn representative_metric() -> Metric {
    Metric::new(
        "http_request_duration_seconds",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 0.042 },
    )
    .with_namespace(Some("http"))
    .with_timestamp(Some(timestamp()))
    .with_tags(Some(metric_tags!(
        "service" => "checkout",
        "host" => "web-01",
        "region" => "us-east-1",
        "route" => "/api/v1/checkout",
        "method" => "POST",
        "status_code" => "200",
    )))
}

fn minimal_trace() -> TraceEvent {
    TraceEvent::from_parts(
        fields([
            ("trace_id", "4bf92f3577b34da6a3ce929d0e0e4736".into()),
            ("span_id", "00f067aa0ba902b7".into()),
        ]),
        EventMetadata::default(),
    )
}

fn representative_trace() -> TraceEvent {
    TraceEvent::from_parts(
        fields([
            ("trace_id", "4bf92f3577b34da6a3ce929d0e0e4736".into()),
            ("span_id", "00f067aa0ba902b7".into()),
            ("parent_span_id", "b7ad6b7169203331".into()),
            ("service", "checkout".into()),
            ("operation", "POST /api/v1/checkout".into()),
            ("duration_ms", 42_i64.into()),
            ("status", "ok".into()),
            (
                "attributes",
                Value::Object(fields([
                    ("http.method", "POST".into()),
                    ("http.route", "/api/v1/checkout".into()),
                    ("http.status_code", 200_i64.into()),
                    ("peer.service", "payments".into()),
                ])),
            ),
            (
                "resource",
                Value::Object(fields([
                    ("service.name", "checkout".into()),
                    ("service.version", "1.14.2".into()),
                    ("service.instance.id", "checkout-7d8cf9d8f6-r4mqw".into()),
                    ("deployment.environment.name", "production".into()),
                    ("cloud.provider", "aws".into()),
                    ("cloud.region", "us-east-1".into()),
                    ("host.name", "ip-10-0-24-156.ec2.internal".into()),
                ])),
            ),
        ]),
        EventMetadata::default(),
    )
}

fn minimal_logs(length: usize) -> EventArray {
    EventArray::Logs((0..length).map(|_| minimal_log()).collect())
}

fn representative_logs(length: usize) -> EventArray {
    EventArray::Logs((0..length).map(|_| representative_log()).collect())
}

fn minimal_metrics(length: usize) -> EventArray {
    EventArray::Metrics((0..length).map(|_| minimal_metric()).collect())
}

fn representative_metrics(length: usize) -> EventArray {
    EventArray::Metrics((0..length).map(|_| representative_metric()).collect())
}

fn minimal_traces(length: usize) -> EventArray {
    EventArray::Traces((0..length).map(|_| minimal_trace()).collect())
}

fn representative_traces(length: usize) -> EventArray {
    EventArray::Traces((0..length).map(|_| representative_trace()).collect())
}

fn disk_capacity(record: &EventArray, records: usize) -> u64 {
    let mut encoded = BytesMut::new();
    record
        .clone()
        .encode(&mut encoded)
        .expect("fixture should encode");
    u64::try_from(encoded.len())
        .expect("encoded fixture must fit in u64")
        .checked_mul(u64::try_from(records).expect("record count must fit in u64"))
        .and_then(|size| size.checked_mul(DISK_CAPACITY_MULTIPLIER))
        .expect("disk capacity must fit in u64")
        .max(MIN_DISK_BUFFER_SIZE)
}

fn memory_capacity(record: &EventArray, records: usize) -> usize {
    record
        .allocated_bytes()
        .checked_mul(records)
        .expect("memory capacity must fit in usize")
}

fn benchmark_operation(c: &mut Criterion, operation: Operation) {
    for buffer_kind in BufferKind::ALL {
        let group_name = format!("native-events-{}", buffer_kind.name());
        let mut group: BenchmarkGroup<WallTime> = c.benchmark_group(&group_name);
        group.sampling_mode(SamplingMode::Auto);
        init_instrumentation();

        let mut data_dir = DataDir::new(&group_name);
        let rt = Runtime::new().expect("could not create Tokio runtime");

        for fixture in FIXTURES {
            for array_length in ARRAY_LENGTHS {
                let record_count = TOTAL_EVENTS / array_length;
                let record = (fixture.make_array)(array_length);
                let disk_bytes = disk_capacity(&record, record_count);
                let memory_bytes = memory_capacity(&record, record_count);
                let json_bytes = record
                    .estimated_json_encoded_size_of()
                    .get()
                    .checked_mul(record_count)
                    .and_then(|size| u64::try_from(size).ok())
                    .expect("JSON-encoded throughput must fit in u64");
                let input = format!(
                    "{}/{}/{}",
                    fixture.event_type, fixture.profile, array_length
                );

                group.throughput(Throughput::Bytes(json_bytes));
                group.bench_with_input(
                    BenchmarkId::new(operation.name(), input),
                    &(
                        fixture,
                        array_length,
                        record_count,
                        memory_bytes,
                        disk_bytes,
                    ),
                    |b, &(fixture, array_length, record_count, memory_bytes, disk_bytes)| {
                        b.to_async(&rt).iter_batched(
                            || {
                                let data_dir = data_dir.next();
                                let id = format!(
                                    "{}-{}-{}-{}",
                                    group_name, fixture.event_type, fixture.profile, array_length
                                );
                                let variant =
                                    buffer_kind.create(TOTAL_EVENTS, memory_bytes, disk_bytes);
                                tokio::task::block_in_place(move || {
                                    Handle::current().block_on(async move {
                                        BenchmarkState::setup_with(
                                            variant,
                                            record_count,
                                            Some(data_dir),
                                            id,
                                            |_| (fixture.make_array)(array_length),
                                        )
                                        .await
                                    })
                                })
                            },
                            |state| operation.measure(state),
                            // Disk buffers allocate a minimum-sized backing file. Avoid batching
                            // multiple setup calls so disk use stays bounded to one buffer.
                            if matches!(buffer_kind, BufferKind::Disk) {
                                BatchSize::PerIteration
                            } else {
                                BatchSize::SmallInput
                            },
                        );
                    },
                );
            }
        }
    }
}

fn write_then_read(c: &mut Criterion) {
    benchmark_operation(c, Operation::WriteThenRead);
}

fn write_and_read(c: &mut Criterion) {
    benchmark_operation(c, Operation::WriteAndRead);
}

criterion_group!(
    name = native_events;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(60))
        .confidence_level(0.99)
        .nresamples(500_000)
        .sample_size(100);
    targets = write_then_read, write_and_read
);
