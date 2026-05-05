//! Benchmarks comparing OTLP encoding approaches
//!
//! Compares the FULL PIPELINE cost for OTLP encoding:
//!
//! 1. **NEW (this PR)**: Native log → automatic OTLP conversion → encode
//! 2. **OLD VRL approach**: Native log → manual OTLP structure build → encode
//!    (simulates what users had to do before this PR)
//! 3. **OLD passthrough**: Pre-formatted OTLP → direct encode (best-case old)

use std::time::Duration;

use bytes::BytesMut;
use criterion::{Criterion, Throughput, criterion_group};
use tokio_util::codec::Encoder;
use vector::event::{Event, LogEvent};
use vector_lib::{
    btreemap,
    byte_size_of::ByteSizeOf,
    codecs::encoding::{OtlpSerializerConfig, Serializer},
};
use vrl::value::{ObjectMap, Value};

// ============================================================================
// TEST DATA
// ============================================================================

/// Native flat log format - what users work with day-to-day
fn create_native_log() -> LogEvent {
    let mut log = LogEvent::from(btreemap! {
        "message" => "User authentication successful",
        "severity_text" => "INFO",
        "severity_number" => 9i64,
    });

    log.insert("attributes.user_id", "user-12345");
    log.insert("attributes.request_id", "req-abc-123");
    log.insert("attributes.duration_ms", 42.5f64);
    log.insert("attributes.success", true);

    log.insert("resources.service.name", "auth-service");
    log.insert("resources.service.version", "2.1.0");
    log.insert("resources.host.name", "prod-server-01");

    log.insert("trace_id", "0123456789abcdef0123456789abcdef");
    log.insert("span_id", "fedcba9876543210");

    log.insert("scope.name", "auth-module");
    log.insert("scope.version", "1.0.0");

    log
}

/// Simulate VRL transformation: build OTLP structure from native log
/// This is what users HAD TO DO before this PR with 50+ lines of VRL
fn simulate_vrl_transform(native_log: &LogEvent) -> LogEvent {
    let mut log = LogEvent::default();

    let mut resource_log = ObjectMap::new();

    // Extract and rebuild resource attributes
    let mut resource = ObjectMap::new();
    let mut resource_attrs = Vec::new();
    if let Some(Value::Object(resources)) = native_log.get("resources") {
        for (k, v) in resources.iter() {
            resource_attrs.push(build_kv_attr(k.as_str(), v.clone()));
        }
    }
    resource.insert("attributes".into(), Value::Array(resource_attrs));
    resource_log.insert("resource".into(), Value::Object(resource));

    // Build scope
    let mut scope_log = ObjectMap::new();
    let mut scope = ObjectMap::new();
    if let Some(name) = native_log.get("scope.name") {
        scope.insert("name".into(), name.clone());
    }
    if let Some(version) = native_log.get("scope.version") {
        scope.insert("version".into(), version.clone());
    }
    scope_log.insert("scope".into(), Value::Object(scope));

    // Build log record
    let mut log_record = ObjectMap::new();
    log_record.insert("timeUnixNano".into(), Value::from("1704067200000000000"));

    if let Some(sev) = native_log.get("severity_text") {
        log_record.insert("severityText".into(), sev.clone());
    }
    if let Some(sev_num) = native_log.get("severity_number") {
        log_record.insert("severityNumber".into(), sev_num.clone());
    }

    // Build body
    let mut body = ObjectMap::new();
    if let Some(msg) = native_log.get("message") {
        if let Value::Bytes(b) = msg {
            body.insert("stringValue".into(), Value::Bytes(b.clone()));
        }
    }
    log_record.insert("body".into(), Value::Object(body));

    // Build attributes
    let mut attrs = Vec::new();
    if let Some(Value::Object(attributes)) = native_log.get("attributes") {
        for (k, v) in attributes.iter() {
            attrs.push(build_kv_attr(k.as_str(), v.clone()));
        }
    }
    log_record.insert("attributes".into(), Value::Array(attrs));

    // Trace context
    if let Some(tid) = native_log.get("trace_id") {
        log_record.insert("traceId".into(), tid.clone());
    }
    if let Some(sid) = native_log.get("span_id") {
        log_record.insert("spanId".into(), sid.clone());
    }

    scope_log.insert("logRecords".into(), Value::Array(vec![Value::Object(log_record)]));
    resource_log.insert("scopeLogs".into(), Value::Array(vec![Value::Object(scope_log)]));
    log.insert("resourceLogs", Value::Array(vec![Value::Object(resource_log)]));

    log
}

fn build_kv_attr(key: &str, value: Value) -> Value {
    let mut attr = ObjectMap::new();
    attr.insert("key".into(), Value::from(key));

    let mut val = ObjectMap::new();
    match value {
        Value::Bytes(b) => {
            val.insert("stringValue".into(), Value::Bytes(b));
        }
        Value::Integer(i) => {
            val.insert("intValue".into(), Value::from(i.to_string()));
        }
        Value::Float(f) => {
            val.insert("doubleValue".into(), Value::Float(f));
        }
        Value::Boolean(b) => {
            val.insert("boolValue".into(), Value::Boolean(b));
        }
        _ => {
            val.insert("stringValue".into(), Value::from(format!("{:?}", value)));
        }
    }
    attr.insert("value".into(), Value::Object(val));
    Value::Object(attr)
}

fn create_preformatted_otlp_log() -> LogEvent {
    let native = create_native_log();
    simulate_vrl_transform(&native)
}

fn create_large_native_log() -> LogEvent {
    let mut log = LogEvent::from(btreemap! {
        "message" => "Detailed request processing log with extensive context",
        "severity_text" => "DEBUG",
        "severity_number" => 5i64,
    });

    for i in 0..50 {
        log.insert(format!("attributes.field_{i}").as_str(), format!("value_{i}"));
    }
    for i in 0..20 {
        log.insert(format!("resources.res_{i}").as_str(), format!("res_value_{i}"));
    }

    log.insert("resources.service.name", "benchmark-service");
    log.insert("trace_id", "0123456789abcdef0123456789abcdef");
    log.insert("span_id", "fedcba9876543210");

    log
}

fn build_otlp_serializer() -> Serializer {
    OtlpSerializerConfig::default()
        .build()
        .expect("Failed to build OTLP serializer")
        .into()
}

// ============================================================================
// BENCHMARKS
// ============================================================================

fn otlp(c: &mut Criterion) {
    let mut group = c.benchmark_group("otlp_encoding");

    let native_log = create_native_log();
    let preformatted_log = create_preformatted_otlp_log();
    let event_size = preformatted_log.size_of() as u64;

    // ========================================================================
    // SINGLE EVENT COMPARISON
    // Serializer and BytesMut are reused across iterations to avoid measuring
    // one-time setup and allocation costs.
    // ========================================================================
    group.throughput(Throughput::Bytes(event_size));

    // NEW: Native → auto-convert → encode
    let native_event = Event::Log(native_log.clone());
    {
        let mut encoder = build_otlp_serializer();
        let mut bytes = BytesMut::with_capacity(4096);
        group.bench_function("1_NEW_auto_convert", |b| {
            b.iter(|| {
                bytes.clear();
                encoder.encode(native_event.clone(), &mut bytes).unwrap();
            })
        });
    }

    // OLD: VRL transform + encode (full pipeline)
    let native_for_vrl = native_log.clone();
    {
        let mut encoder = build_otlp_serializer();
        let mut bytes = BytesMut::with_capacity(4096);
        group.bench_function("2_OLD_vrl_transform_encode", |b| {
            b.iter(|| {
                bytes.clear();
                let transformed = simulate_vrl_transform(&native_for_vrl);
                encoder.encode(Event::Log(transformed), &mut bytes).unwrap();
            })
        });
    }

    // OLD: Passthrough only (encode only, no transform)
    let preformatted = Event::Log(preformatted_log.clone());
    {
        let mut encoder = build_otlp_serializer();
        let mut bytes = BytesMut::with_capacity(4096);
        group.bench_function("3_OLD_passthrough_only", |b| {
            b.iter(|| {
                bytes.clear();
                encoder.encode(preformatted.clone(), &mut bytes).unwrap();
            })
        });
    }

    // ========================================================================
    // BATCH COMPARISON (Production Scenario)
    // ========================================================================
    let batch: Vec<LogEvent> = (0..100).map(|_| create_native_log()).collect();
    let batch_size: u64 = batch.iter().map(|e| e.size_of() as u64).sum();
    group.throughput(Throughput::Bytes(batch_size));

    {
        let mut encoder = build_otlp_serializer();
        let mut bytes = BytesMut::with_capacity(4096);
        group.bench_function("4_NEW_batch_100", |b| {
            b.iter(|| {
                for log in batch.iter() {
                    bytes.clear();
                    encoder.encode(Event::Log(log.clone()), &mut bytes).unwrap();
                }
            })
        });
    }

    {
        let mut encoder = build_otlp_serializer();
        let mut bytes = BytesMut::with_capacity(4096);
        group.bench_function("5_OLD_batch_100_vrl", |b| {
            b.iter(|| {
                for log in batch.iter() {
                    bytes.clear();
                    let transformed = simulate_vrl_transform(log);
                    encoder.encode(Event::Log(transformed), &mut bytes).unwrap();
                }
            })
        });
    }

    // ========================================================================
    // LARGE EVENT (Stress Test)
    // ========================================================================
    let large_log = Event::Log(create_large_native_log());
    group.throughput(Throughput::Bytes(large_log.size_of() as u64));

    {
        let mut encoder = build_otlp_serializer();
        let mut bytes = BytesMut::with_capacity(4096);
        group.bench_function("6_NEW_large_70_attrs", |b| {
            b.iter(|| {
                bytes.clear();
                encoder.encode(large_log.clone(), &mut bytes).unwrap();
            })
        });
    }

    group.finish();
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(3))
        .measurement_time(Duration::from_secs(10))
        .noise_threshold(0.02)
        .significance_level(0.05)
        .confidence_level(0.95)
        .nresamples(50_000)
        .sample_size(50);
    targets = otlp
);
