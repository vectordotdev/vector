// use super::*;
use super::config::HecMetricsSinkConfig;
use super::sink::{process_metric, HecProcessedEvent};
use crate::event::{Metric, MetricKind, MetricValue};
// use crate::sinks::util::{http::HttpSink, test::load_sink};
use crate::template::Template;
use chrono::{DateTime, Utc};
use shared::btreemap;
use std::convert::TryFrom;
use vector_core::ByteSizeOf;

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

fn get_processed_event(metric: Metric) -> HecProcessedEvent {
    let sourcetype = Template::try_from("{{ tags.template_sourcetype }}".to_string()).ok();
    let source = Template::try_from("{{ tags.template_source }}".to_string()).ok();
    let index = Template::try_from("{{ tags.template_index }}".to_string()).ok();
    let default_namespace = Some("namespace");
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

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<HecMetricsSinkConfig>();
}

#[test]
fn splunk_process_metrics_event() {
    let metric = get_counter();
    let processed_event = get_processed_event(metric);
    let metadata = processed_event.metadata;

    assert_eq!(metadata.sourcetype, Some("sourcetype_value".to_string()));
    assert_eq!(metadata.source, Some("source_value".to_string()));
    assert_eq!(metadata.index, Some("index_value".to_string()));
    assert_eq!(metadata.host, Some("host_value".to_string()));
}

// #[test]
// fn test_encode_event_templated_counter_returns_expected_json() {
//     // let timestamp = DateTime::parse_from_rfc3339("2005-12-12T14:12:55.123-00:00")
//     //     .unwrap()
//     //     .with_timezone(&Utc);

//     let metric = get_counter();
//     // Metric::new(
//     //     "example-counter",
//     //     MetricKind::Absolute,
//     //     MetricValue::Counter { value: 26.8 },
//     // )
//     // .with_timestamp(Some(timestamp))
//     // .with_tags(Some(btreemap! {
//     //     "template_index".to_string() => "index_value".to_string(),
//     //     "template_source".to_string() => "source_value".to_string(),
//     //     "template_sourcetype".to_string() => "sourcetype_value".to_string(),
//     //     "tag_one".to_string() => "tag_one_value".to_string(),
//     //     "tag_two".to_string() => "tag_two_value".to_string(),
//     //     "host".to_string() => "host_value".to_string(),
//     // }));

//     let (config, _cx) = load_sink::<HecSinkMetricsConfig>(
//         r#"
//         endpoint = "https://splunk-hec.com/"
//         token = "alksjdfo"
//         host_key = "host"
//         index = "{{ tags.template_index }}"
//         source = "{{ tags.template_source }}"
//         sourcetype = "{{ tags.template_sourcetype }}"
//     "#,
//     )
//     .unwrap();

//     let expected = json!({
//         "time": 1134396775.123,
//         "host": "host_value",
//         "index": "index_value",
//         "source": "source_value",
//         "sourcetype": "sourcetype_value",
//         "fields": {
//             "host": "host_value",
//             "tag_one": "tag_one_value",
//             "tag_two": "tag_two_value",
//             "metric_name": "example-counter",
//             "_value": 26.8,
//         },
//         "event": "metric",
//     });

//     let actual =
//         serde_json::from_slice::<JsonValue>(&config.encode_event(metric.into()).unwrap()[..])
//             .unwrap();

//     assert_eq!(expected, actual);
// }

// #[test]
// fn test_encode_event_static_counter_returns_expected_json() {
//     let timestamp = DateTime::parse_from_rfc3339("2005-12-12T14:12:55.123-00:00")
//         .unwrap()
//         .with_timezone(&Utc);

//     let metric = Metric::new(
//         "example-counter",
//         MetricKind::Absolute,
//         MetricValue::Counter { value: 26.8 },
//     )
//     .with_timestamp(Some(timestamp))
//     .with_tags(Some(btreemap! {
//         "template_index".to_string() => "index_value".to_string(),
//         "template_source".to_string() => "source_value".to_string(),
//         "template_sourcetype".to_string() => "sourcetype_value".to_string(),
//         "tag_one".to_string() => "tag_one_value".to_string(),
//         "tag_two".to_string() => "tag_two_value".to_string(),
//         "host".to_string() => "host_value".to_string(),
//     }));

//     let (config, _cx) = load_sink::<HecSinkMetricsConfig>(
//         r#"
//         endpoint = "https://splunk-hec.com/"
//         token = "alksjdfo"
//         host_key = "host"
//         index = "index_value"
//         source = "source_value"
//         sourcetype = "sourcetype_value"
//     "#,
//     )
//     .unwrap();

//     let expected = json!({
//         "time": 1134396775.123,
//         "host": "host_value",
//         "index": "index_value",
//         "source": "source_value",
//         "sourcetype": "sourcetype_value",
//         "fields": {
//             "host": "host_value",
//             "tag_one": "tag_one_value",
//             "tag_two": "tag_two_value",
//             "template_index": "index_value",
//             "template_source": "source_value",
//             "template_sourcetype": "sourcetype_value",
//             "metric_name": "example-counter",
//             "_value": 26.8,
//         },
//         "event": "metric",
//     });

//     let actual =
//         serde_json::from_slice::<JsonValue>(&config.encode_event(metric.into()).unwrap()[..])
//             .unwrap();

//     assert_eq!(expected, actual);
// }

// #[test]
// fn test_encode_event_gauge_no_namespace_returns_expected_json() {
//     let timestamp = DateTime::parse_from_rfc3339("2005-12-12T14:12:55.123-00:00")
//         .unwrap()
//         .with_timezone(&Utc);

//     let metric = Metric::new(
//         "example-gauge",
//         MetricKind::Absolute,
//         MetricValue::Gauge { value: 26.8 },
//     )
//     .with_timestamp(Some(timestamp));

//     let (config, _cx) = load_sink::<HecSinkMetricsConfig>(
//         r#"
//         endpoint = "https://splunk-hec.com/"
//         token = "alksjdfo"
//     "#,
//     )
//     .unwrap();

//     let expected = json!({
//         "time": 1134396775.123,
//         "fields": {
//             "metric_name": "example-gauge",
//             "_value": 26.8,
//         },
//         "event": "metric",
//     });

//     let actual =
//         serde_json::from_slice::<JsonValue>(&config.encode_event(metric.into()).unwrap()[..])
//             .unwrap();

//     assert_eq!(expected, actual);
// }

// #[test]
// fn test_encode_event_gauge_with_namespace_returns_expected_json() {
//     let timestamp = DateTime::parse_from_rfc3339("2005-12-12T14:12:55.123-00:00")
//         .unwrap()
//         .with_timezone(&Utc);

//     let metric = Metric::new(
//         "example-gauge",
//         MetricKind::Absolute,
//         MetricValue::Gauge { value: 26.8 },
//     )
//     .with_timestamp(Some(timestamp))
//     .with_namespace(Some("namespace"));

//     let (config, _cx) = load_sink::<HecSinkMetricsConfig>(
//         r#"
//         endpoint = "https://splunk-hec.com/"
//         token = "alksjdfo"
//     "#,
//     )
//     .unwrap();

//     let expected = json!({
//         "time": 1134396775.123,
//         "fields": {
//             "metric_name": "namespace.example-gauge",
//             "_value": 26.8,
//         },
//         "event": "metric",
//     });

//     let actual =
//         serde_json::from_slice::<JsonValue>(&config.encode_event(metric.into()).unwrap()[..])
//             .unwrap();

//     assert_eq!(expected, actual);
// }

// #[test]
// fn test_encode_event_gauge_default_namespace_returns_expected_json() {
//     let timestamp = DateTime::parse_from_rfc3339("2005-12-12T14:12:55.123-00:00")
//         .unwrap()
//         .with_timezone(&Utc);

//     let metric = Metric::new(
//         "example-gauge",
//         MetricKind::Absolute,
//         MetricValue::Gauge { value: 26.8 },
//     )
//     .with_timestamp(Some(timestamp));

//     let (config, _cx) = load_sink::<HecSinkMetricsConfig>(
//         r#"
//         default_namespace = "default"
//         endpoint = "https://splunk-hec.com/"
//         token = "alksjdfo"
//     "#,
//     )
//     .unwrap();

//     let expected = json!({
//         "time": 1134396775.123,
//         "fields": {
//             "metric_name": "default.example-gauge",
//             "_value": 26.8,
//         },
//         "event": "metric",
//     });

//     let actual =
//         serde_json::from_slice::<JsonValue>(&config.encode_event(metric.into()).unwrap()[..])
//             .unwrap();

//     assert_eq!(expected, actual);
// }

// #[test]
// fn test_encode_event_gauge_overridden_namespace_returns_expected_json() {
//     let timestamp = DateTime::parse_from_rfc3339("2005-12-12T14:12:55.123-00:00")
//         .unwrap()
//         .with_timezone(&Utc);

//     let metric = Metric::new(
//         "example-gauge",
//         MetricKind::Absolute,
//         MetricValue::Gauge { value: 26.8 },
//     )
//     .with_timestamp(Some(timestamp))
//     .with_namespace(Some("overridden"));

//     let (config, _cx) = load_sink::<HecSinkMetricsConfig>(
//         r#"
//         default_namespace = "default"
//         endpoint = "https://splunk-hec.com/"
//         token = "alksjdfo"
//     "#,
//     )
//     .unwrap();

//     let expected = json!({
//         "time": 1134396775.123,
//         "fields": {
//             "metric_name": "overridden.example-gauge",
//             "_value": 26.8,
//         },
//         "event": "metric",
//     });

//     let actual =
//         serde_json::from_slice::<JsonValue>(&config.encode_event(metric.into()).unwrap()[..])
//             .unwrap();

//     assert_eq!(expected, actual);
// }

// #[test]
// fn test_encode_event_unsupported_type_returns_none() {
//     let mut values = BTreeSet::new();
//     values.insert(String::from("value1"));

//     let metric = Metric::new(
//         "example-gauge",
//         MetricKind::Absolute,
//         MetricValue::Set { values },
//     );

//     let (config, _cx) = load_sink::<HecSinkMetricsConfig>(
//         r#"
//         endpoint = "https://splunk-hec.com/"
//         token = "alksjdfo"
//     "#,
//     )
//     .unwrap();

//     assert!(config.encode_event(metric.into()).is_none());
// }
