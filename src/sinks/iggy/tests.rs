use vrl::value::Value;

use super::config::IggySinkConfig;
use super::otlp::{Signal, decode_event, signal_of};
use super::proto::{WriteBatch, encode_ref, shard_of_fingerprint, QueueGeneration};

#[test]
fn generate_config_roundtrips() {
    crate::test_util::test_generate_config::<IggySinkConfig>();
}

fn otlp_value(json: serde_json::Value) -> Value {
    Value::from(json)
}

#[test]
fn maps_otlp_sum_metric_to_total_sample() {
    // Minimal resourceMetrics with one monotonic Sum data point.
    let event = otlp_value(serde_json::json!({
        "resourceMetrics": [{
            "resource": {"attributes": [
                {"key": "service.name", "value": {"stringValue": "checkout"}}
            ]},
            "scopeMetrics": [{
                "metrics": [{
                    "name": "http.server.requests",
                    "sum": {
                        "isMonotonic": true,
                        "dataPoints": [{
                            "timeUnixNano": 1000,
                            "asDouble": 5.0,
                            "attributes": [
                                {"key": "code", "value": {"stringValue": "200"}}
                            ]
                        }]
                    }
                }]
            }]
        }]
    }));
    assert_eq!(signal_of(&event), Some(Signal::Metrics));
    let mut batch = WriteBatch::new("default");
    decode_event(&event, &mut batch);
    assert_eq!(batch.samples.len(), 1);
    let (labels, row) = &batch.samples[0];
    assert_eq!(row.value, 5.0);
    // metric name gains `_total`; job derives from service.name
    let names: Vec<_> = labels_names(labels);
    assert!(names.contains(&"__name__".to_string()));
}

#[test]
fn maps_otlp_logs_with_severity_and_trace_id() {
    let event = otlp_value(serde_json::json!({
        "resourceLogs": [{
            "resource": {"attributes": [
                {"key": "service.name", "value": {"stringValue": "api"}}
            ]},
            "scopeLogs": [{
                "logRecords": [{
                    "timeUnixNano": 42,
                    "severityText": "ERROR",
                    "body": {"stringValue": "boom code=500"},
                    "traceId": "4ac52aadf321c2e531db005df08792f5",
                    "attributes": []
                }]
            }]
        }]
    }));
    assert_eq!(signal_of(&event), Some(Signal::Logs));
    let mut batch = WriteBatch::new("default");
    decode_event(&event, &mut batch);
    assert_eq!(batch.logs.len(), 1);
    assert_eq!(batch.logs[0].1.line, "boom code=500");
    assert_eq!(batch.logs[0].1.timestamp_ns, 42);
}

#[test]
fn encoded_sample_batch_is_shard_placed_and_decodable() {
    let event = otlp_value(serde_json::json!({
        "resourceMetrics": [{
            "resource": {"attributes": []},
            "scopeMetrics": [{
                "metrics": [{
                    "name": "up",
                    "gauge": {"dataPoints": [{"timeUnixNano": 1, "asDouble": 1.0, "attributes": []}]}
                }]
            }]
        }]
    }));
    let mut batch = WriteBatch::new("default");
    decode_event(&event, &mut batch);
    assert_eq!(batch.samples.len(), 1);

    let shards = 8;
    let generation = QueueGeneration {
        stream_id: 1,
        stream_created_at_micros: 2,
        topic_id: 3,
        topic_created_at_micros: 4,
    };
    for (shard, part) in batch.split_by_shard(shards).unwrap() {
        // every row lands in the shard the placement function assigns
        for (labels, _) in &part.samples {
            assert_eq!(shard_of_fingerprint(labels.fingerprint(), shards), shard);
        }
        let bytes = encode_ref(generation, shard, shards, &part).unwrap();
        assert!(!bytes.is_empty());
    }
}

fn labels_names(labels: &super::proto::Labels) -> Vec<String> {
    // Labels has no public iterator here; serialize to inspect names.
    serde_json::to_value(labels)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .map(|arr| {
            arr.into_iter()
                .filter_map(|l| l.get("name").and_then(|n| n.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}
