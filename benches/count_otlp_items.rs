use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use vector::event::{Event, LogEvent, TraceEvent};
use vector::sources::opentelemetry::count_otlp_items;
use vector_lib::opentelemetry::proto::{
    RESOURCE_LOGS_JSON_FIELD, RESOURCE_METRICS_JSON_FIELD, RESOURCE_SPANS_JSON_FIELD,
};
use vrl::value;

/// Generate a batch of log events with specified number of log records
fn generate_log_batch(total_records: usize) -> Vec<Event> {
    let records_per_scope = total_records.max(1);

    let log_records: Vec<_> = (0..records_per_scope)
        .map(|i| {
            let body_text = format!("log message {}", i);
            value!({
                "timeUnixNano": "1234567890000000000",
                "observedTimeUnixNano": "1234567890000000000",
                "severityNumber": 9,
                "severityText": "INFO",
                "body": {"stringValue": body_text},
                "attributes": [
                    {"key": "attr1", "value": {"stringValue": "value1"}}
                ]
            })
        })
        .collect();

    let mut log = LogEvent::default();
    log.insert(
        RESOURCE_LOGS_JSON_FIELD,
        value!([{
            "resource": {
                "attributes": [
                    {"key": "service.name", "value": {"stringValue": "test-service"}}
                ]
            },
            "scopeLogs": [{
                "scope": {
                    "name": "test.scope",
                    "version": "1.0.0"
                },
                "logRecords": log_records
            }]
        }]),
    );

    vec![Event::Log(log)]
}

/// Generate a batch with multiple resources and scopes (nested structure)
fn generate_nested_log_batch(num_resources: usize, num_scopes_per_resource: usize) -> Vec<Event> {
    let mut log = LogEvent::default();

    let resource_logs: Vec<_> = (0..num_resources)
        .map(|r| {
            let scope_logs: Vec<_> = (0..num_scopes_per_resource)
                .map(|s| {
                    let scope_name = format!("scope.{}.{}", r, s);
                    let body_text = format!("log from resource {} scope {}", r, s);
                    value!({
                        "scope": {
                            "name": scope_name,
                            "version": "1.0.0"
                        },
                        "logRecords": [
                            {
                                "timeUnixNano": "1234567890000000000",
                                "severityNumber": 9,
                                "body": {"stringValue": body_text}
                            }
                        ]
                    })
                })
                .collect();

            let resource_id = format!("resource-{}", r);
            value!({
                "resource": {
                    "attributes": [
                        {"key": "resource.id", "value": {"stringValue": resource_id}}
                    ]
                },
                "scopeLogs": scope_logs
            })
        })
        .collect();

    log.insert(RESOURCE_LOGS_JSON_FIELD, value!(resource_logs));

    vec![Event::Log(log)]
}

/// Generate a batch of metric events
fn generate_metric_batch(total_metrics: usize) -> Vec<Event> {
    let mut log = LogEvent::default();

    let metrics: Vec<_> = (0..total_metrics)
        .map(|i| {
            let metric_name = format!("metric_{}", i);
            value!({
                "name": metric_name,
                "description": "test metric",
                "unit": "1",
                "sum": {
                    "dataPoints": [{
                        "asInt": "42",
                        "timeUnixNano": "1234567890000000000"
                    }],
                    "aggregationTemporality": 2,
                    "isMonotonic": true
                }
            })
        })
        .collect();

    log.insert(
        RESOURCE_METRICS_JSON_FIELD,
        value!([{
            "resource": {
                "attributes": [
                    {"key": "service.name", "value": {"stringValue": "test-service"}}
                ]
            },
            "scopeMetrics": [{
                "scope": {
                    "name": "test.metrics",
                    "version": "1.0.0"
                },
                "metrics": metrics
            }]
        }]),
    );

    vec![Event::Log(log)]
}

/// Generate a batch of trace events
fn generate_trace_batch(total_spans: usize) -> Vec<Event> {
    let mut trace = TraceEvent::default();

    let spans: Vec<_> = (0..total_spans)
        .map(|i| {
            let trace_id = format!("{:032x}", i);
            let span_id = format!("{:016x}", i);
            let span_name = format!("span_{}", i);
            value!({
                "traceId": trace_id,
                "spanId": span_id,
                "name": span_name,
                "kind": 1,
                "startTimeUnixNano": "1234567890000000000",
                "endTimeUnixNano": "1234567890000000000",
                "attributes": [
                    {"key": "span.id", "value": {"intValue": i}}
                ]
            })
        })
        .collect();

    trace.insert(
        RESOURCE_SPANS_JSON_FIELD,
        value!([{
            "resource": {
                "attributes": [
                    {"key": "service.name", "value": {"stringValue": "test-service"}}
                ]
            },
            "scopeSpans": [{
                "scope": {
                    "name": "test.traces",
                    "version": "1.0.0"
                },
                "spans": spans
            }]
        }]),
    );

    vec![Event::Trace(trace)]
}

/// Generate a mixed batch with logs, metrics, and traces
fn generate_mixed_batch(items_per_type: usize) -> Vec<Event> {
    let mut events = Vec::new();
    events.extend(generate_log_batch(items_per_type));
    events.extend(generate_metric_batch(items_per_type));
    events.extend(generate_trace_batch(items_per_type));
    events
}

/// Generate batch with empty or malformed structures
fn generate_edge_case_batch() -> Vec<Event> {
    let mut events = Vec::new();

    // Empty resourceLogs
    let mut log1 = LogEvent::default();
    log1.insert(RESOURCE_LOGS_JSON_FIELD, value!([]));
    events.push(Event::Log(log1));

    // Empty scopeLogs
    let mut log2 = LogEvent::default();
    log2.insert(RESOURCE_LOGS_JSON_FIELD, value!([{"scopeLogs": []}]));
    events.push(Event::Log(log2));

    // Empty logRecords
    let mut log3 = LogEvent::default();
    log3.insert(
        RESOURCE_LOGS_JSON_FIELD,
        value!([{"scopeLogs": [{"logRecords": []}]}]),
    );
    events.push(Event::Log(log3));

    // Non-OTLP event (should count as 0)
    let log4 = LogEvent::default();
    events.push(Event::Log(log4));

    events
}

fn benchmark_batch_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("count_otlp_items/batch_sizes");

    for size in [1, 10, 100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::new("logs", size), size, |b, &size| {
            b.iter_batched(
                || generate_log_batch(size),
                |events| count_otlp_items(&events),
                BatchSize::SmallInput,
            )
        });

        group.bench_with_input(BenchmarkId::new("metrics", size), size, |b, &size| {
            b.iter_batched(
                || generate_metric_batch(size),
                |events| count_otlp_items(&events),
                BatchSize::SmallInput,
            )
        });

        group.bench_with_input(BenchmarkId::new("traces", size), size, |b, &size| {
            b.iter_batched(
                || generate_trace_batch(size),
                |events| count_otlp_items(&events),
                BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

fn benchmark_nested_complexity(c: &mut Criterion) {
    let mut group = c.benchmark_group("count_otlp_items/nested");

    // Test different nesting depths
    for resources in [1, 5, 10].iter() {
        for scopes in [1, 5, 10].iter() {
            let total_items = resources * scopes;
            group.throughput(Throughput::Elements(total_items as u64));

            group.bench_with_input(
                BenchmarkId::from_parameter(format!("{}r_{}s", resources, scopes)),
                &(*resources, *scopes),
                |b, &(r, s)| {
                    b.iter_batched(
                        || generate_nested_log_batch(r, s),
                        |events| count_otlp_items(&events),
                        BatchSize::SmallInput,
                    )
                },
            );
        }
    }

    group.finish();
}

fn benchmark_mixed_workload(c: &mut Criterion) {
    let mut group = c.benchmark_group("count_otlp_items/mixed");

    for size in [10, 100, 1000].iter() {
        let total_items = size * 3; // logs + metrics + traces
        group.throughput(Throughput::Elements(total_items as u64));

        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter_batched(
                || generate_mixed_batch(size),
                |events| count_otlp_items(&events),
                BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

fn benchmark_edge_cases(c: &mut Criterion) {
    let mut group = c.benchmark_group("count_otlp_items/edge_cases");

    group.bench_function("empty_and_malformed", |b| {
        b.iter_batched(
            generate_edge_case_batch,
            |events| count_otlp_items(&events),
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_batch_sizes,
    benchmark_nested_complexity,
    benchmark_mixed_workload,
    benchmark_edge_cases
);
criterion_main!(benches);
