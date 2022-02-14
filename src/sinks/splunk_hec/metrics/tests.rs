use std::{collections::BTreeSet, sync::Arc};

use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use serde_json::{json, Value as JsonValue};
use vector_common::btreemap;
use vector_core::{
    event::{Event, Metric, MetricKind, MetricValue},
    ByteSizeOf,
};

use super::sink::{process_metric, HecProcessedEvent};
use crate::{
    config::{SinkConfig, SinkContext},
    sinks::{
        splunk_hec::metrics::{config::HecMetricsSinkConfig, encoder::HecMetricsEncoder},
        util::{test::build_test_server, Compression},
    },
    template::Template,
    test_util::next_addr,
};

fn get_counter() -> Metric {
    let timestamp = DateTime::parse_from_rfc3339("2005-12-12T14:12:55.123-00:00")
        .unwrap()
        .with_timezone(&Utc);

    Metric::new(
        "example-counter",
        MetricKind::Absolute,
        MetricValue::Counter { value: 26.8 },
    )
    .with_timestamp(Some(timestamp))
    .with_tags(Some(btreemap! {
        "template_index".to_string() => "index_value".to_string(),
        "template_source".to_string() => "source_value".to_string(),
        "template_sourcetype".to_string() => "sourcetype_value".to_string(),
        "tag_one".to_string() => "tag_one_value".to_string(),
        "tag_two".to_string() => "tag_two_value".to_string(),
        "host".to_string() => "host_value".to_string(),
    }))
}

fn get_gauge(namespace: Option<String>) -> Metric {
    let timestamp = DateTime::parse_from_rfc3339("2005-12-12T14:12:55.123-00:00")
        .unwrap()
        .with_timezone(&Utc);

    Metric::new(
        "example-gauge",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 26.8 },
    )
    .with_timestamp(Some(timestamp))
    .with_namespace(namespace)
}

fn get_processed_event(
    metric: Metric,
    sourcetype: Option<Template>,
    source: Option<Template>,
    index: Option<Template>,
    default_namespace: Option<&str>,
) -> HecProcessedEvent {
    let event_byte_size = metric.size_of();

    process_metric(
        metric,
        event_byte_size,
        sourcetype.as_ref(),
        source.as_ref(),
        index.as_ref(),
        "host",
        default_namespace,
    )
    .unwrap()
}

fn get_event_with_token(token: &str) -> Event {
    let mut event = Event::from(get_counter());
    event
        .metadata_mut()
        .set_splunk_hec_token(Some(Arc::from(token)));
    event
}

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<HecMetricsSinkConfig>();
}

#[test]
fn test_process_metric() {
    let sourcetype = Template::try_from("{{ tags.template_sourcetype }}".to_string()).ok();
    let source = Template::try_from("{{ tags.template_source }}".to_string()).ok();
    let index = Template::try_from("{{ tags.template_index }}".to_string()).ok();
    let default_namespace = Some("namespace");
    let metric = get_counter();
    let processed_event = get_processed_event(metric, sourcetype, source, index, default_namespace);
    let mut metadata = processed_event.metadata;

    assert_eq!(metadata.sourcetype, Some("sourcetype_value".to_string()));
    assert_eq!(metadata.source, Some("source_value".to_string()));
    assert_eq!(metadata.index, Some("index_value".to_string()));
    assert_eq!(metadata.host, Some("host_value".to_string()));
    assert_eq!(
        metadata.metric_name,
        "namespace.example-counter".to_string()
    );
    assert_eq!(metadata.metric_value, 26.8);
    metadata.templated_field_keys.sort();
    assert_eq!(
        metadata.templated_field_keys.as_slice(),
        ["template_index", "template_source", "template_sourcetype"]
    );
}

#[test]
fn test_process_metric_unsupported_type_returns_none() {
    let mut values = BTreeSet::new();
    values.insert(String::from("value1"));

    let metric = Metric::new(
        "example-set",
        MetricKind::Absolute,
        MetricValue::Set { values },
    );

    let event_byte_size = metric.size_of();
    let sourcetype = None;
    let source = None;
    let index = None;
    let default_namespace = None;
    assert!(process_metric(
        metric,
        event_byte_size,
        sourcetype,
        source,
        index,
        "host_key",
        default_namespace
    )
    .is_none());
}

#[test]
fn test_encode_event_templated_counter_returns_expected_json() {
    let sourcetype = Template::try_from("{{ tags.template_sourcetype }}".to_string()).ok();
    let source = Template::try_from("{{ tags.template_source }}".to_string()).ok();
    let index = Template::try_from("{{ tags.template_index }}".to_string()).ok();
    let default_namespace = Some("namespace");
    let metric = get_counter();
    let processed_event = get_processed_event(metric, sourcetype, source, index, default_namespace);

    let expected = json!({
        "time": 1134396775.123,
        "host": "host_value",
        "index": "index_value",
        "source": "source_value",
        "sourcetype": "sourcetype_value",
        "fields": {
            "host": "host_value",
            "tag_one": "tag_one_value",
            "tag_two": "tag_two_value",
            "metric_name": "namespace.example-counter",
            "_value": 26.8,
        },
        "event": "metric",
    });

    let actual = serde_json::from_slice::<JsonValue>(
        &HecMetricsEncoder::encode_event(processed_event).unwrap()[..],
    )
    .unwrap();

    assert_eq!(expected, actual);
}

#[test]
fn test_encode_event_static_counter_returns_expected_json() {
    let sourcetype = Template::try_from("sourcetype_value".to_string()).ok();
    let source = Template::try_from("source_value".to_string()).ok();
    let index = Template::try_from("index_value".to_string()).ok();
    let default_namespace = None;
    let metric = get_counter();
    let processed_event = get_processed_event(metric, sourcetype, source, index, default_namespace);

    let expected = json!({
        "time": 1134396775.123,
        "host": "host_value",
        "index": "index_value",
        "source": "source_value",
        "sourcetype": "sourcetype_value",
        "fields": {
            "host": "host_value",
            "tag_one": "tag_one_value",
            "tag_two": "tag_two_value",
            "template_index": "index_value",
            "template_source": "source_value",
            "template_sourcetype": "sourcetype_value",
            "metric_name": "example-counter",
            "_value": 26.8,
        },
        "event": "metric",
    });

    let actual = serde_json::from_slice::<JsonValue>(
        &HecMetricsEncoder::encode_event(processed_event).unwrap()[..],
    )
    .unwrap();

    assert_eq!(expected, actual);
}

#[test]
fn test_encode_event_gauge_returns_expected_json() {
    let sourcetype = None;
    let source = None;
    let index = None;
    let default_namespace = None;
    let metric = get_gauge(None);
    let processed_event = get_processed_event(metric, sourcetype, source, index, default_namespace);

    let expected = json!({
        "time": 1134396775.123,
        "fields": {
            "metric_name": "example-gauge",
            "_value": 26.8,
        },
        "event": "metric",
    });

    let actual = serde_json::from_slice::<JsonValue>(
        &HecMetricsEncoder::encode_event(processed_event).unwrap()[..],
    )
    .unwrap();

    assert_eq!(expected, actual);
}

#[test]
fn test_encode_event_gauge_with_namespace_returns_expected_json() {
    let sourcetype = None;
    let source = None;
    let index = None;
    let default_namespace = None;
    let metric = get_gauge(Some("namespace".to_string()));
    let processed_event = get_processed_event(metric, sourcetype, source, index, default_namespace);

    let expected = json!({
        "time": 1134396775.123,
        "fields": {
            "metric_name": "namespace.example-gauge",
            "_value": 26.8,
        },
        "event": "metric",
    });

    let actual = serde_json::from_slice::<JsonValue>(
        &HecMetricsEncoder::encode_event(processed_event).unwrap()[..],
    )
    .unwrap();

    assert_eq!(expected, actual);
}

#[test]
fn test_encode_event_gauge_default_namespace_returns_expected_json() {
    let sourcetype = None;
    let source = None;
    let index = None;
    let default_namespace = Some("default");
    let metric = get_gauge(None);
    let processed_event = get_processed_event(metric, sourcetype, source, index, default_namespace);

    let expected = json!({
        "time": 1134396775.123,
        "fields": {
            "metric_name": "default.example-gauge",
            "_value": 26.8,
        },
        "event": "metric",
    });

    let actual = serde_json::from_slice::<JsonValue>(
        &HecMetricsEncoder::encode_event(processed_event).unwrap()[..],
    )
    .unwrap();

    assert_eq!(expected, actual);
}

#[test]
fn test_encode_event_gauge_overridden_namespace_returns_expected_json() {
    let sourcetype = None;
    let source = None;
    let index = None;
    let default_namespace = Some("default");
    let metric = get_gauge(Some("this_namespace_will_override_the_default".to_string()));
    let processed_event = get_processed_event(metric, sourcetype, source, index, default_namespace);

    let expected = json!({
        "time": 1134396775.123,
        "fields": {
            "metric_name": "this_namespace_will_override_the_default.example-gauge",
            "_value": 26.8,
        },
        "event": "metric",
    });

    let actual = serde_json::from_slice::<JsonValue>(
        &HecMetricsEncoder::encode_event(processed_event).unwrap()[..],
    )
    .unwrap();

    assert_eq!(expected, actual);
}

#[tokio::test]
async fn splunk_passthrough_token() {
    let addr = next_addr();
    let config = HecMetricsSinkConfig {
        default_token: "token".into(),
        endpoint: format!("http://{}", addr),
        host_key: "host".into(),
        index: None,
        sourcetype: None,
        source: None,
        compression: Compression::None,
        batch: Default::default(),
        request: Default::default(),
        tls: None,
        acknowledgements: Default::default(),
        default_namespace: None,
    };
    let cx = SinkContext::new_test();

    let (sink, _) = config.build(cx).await.unwrap();

    let (rx, _trigger, server) = build_test_server(addr);
    tokio::spawn(server);

    let events = vec![
        get_event_with_token("passthrough-token-1"),
        get_event_with_token("passthrough-token-2"),
        Event::from(get_counter()),
    ];

    let _ = sink.run_events(events).await.unwrap();

    let mut tokens = rx
        .take(3)
        .map(|r| r.0.headers.get("Authorization").unwrap().clone())
        .collect::<Vec<_>>()
        .await;

    tokens.sort();
    assert_eq!(
        tokens,
        vec![
            "Splunk passthrough-token-1",
            "Splunk passthrough-token-2",
            "Splunk token"
        ]
    )
}
