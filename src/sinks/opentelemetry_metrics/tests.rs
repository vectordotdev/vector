use chrono::{offset::TimeZone, Timelike, Utc};
use similar_asserts::assert_eq;
use vector_lib::config::proxy::ProxyConfig;
use vector_lib::metric_tags;
use vector_lib::opentelemetry::proto::{
    common::v1::any_value,
    metrics::v1::{metric, number_data_point, AggregationTemporality},
};

use super::*;
use crate::http::HttpClient;
use crate::event::metric::{Metric, MetricKind, MetricValue, StatisticKind};
use crate::sinks::opentelemetry_metrics::metric::Data;

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<OpentelemetryMetricsSinkConfig>();
}

fn config() -> OpentelemetryMetricsSinkConfig {
    OpentelemetryMetricsSinkConfig {
        endpoint: "http://localhost:4317".to_string(),
        healthcheck_endpoint: "http://localhost:13133".to_string(),
        default_namespace: Some("vector".into()),
        aggregation_temporality: AggregationTemporalityConfig::Cumulative,
        compression: Default::default(),
        batch: Default::default(),
        request: Default::default(),
        tls: None,
        encoding: Default::default(),
        acknowledgements: Default::default(),
    }
}

async fn svc() -> OpentelemetryMetricsSvc {
    let config = config();
    let client = HttpClient::new(None, &ProxyConfig::from_env()).unwrap();
    OpentelemetryMetricsSvc { config, client }
}

#[tokio::test]
async fn encode_events_basic_counter() {
    let events = vec![
        Metric::new(
            "exception_total",
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.0 },
        ),
        Metric::new(
            "bytes_out",
            MetricKind::Incremental,
            MetricValue::Counter { value: 2.5 },
        )
        .with_timestamp(Some(
            Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10)
                .single()
                .and_then(|t| t.with_nanosecond(123456789))
                .expect("invalid timestamp"),
        )),
        Metric::new(
            "healthcheck",
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.0 },
        )
        .with_tags(Some(metric_tags!("region" => "local")))
        .with_timestamp(Some(
            Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10)
                .single()
                .and_then(|t| t.with_nanosecond(123456789))
                .expect("invalid timestamp"),
        )),
    ];

    let metrics = svc().await.encode_events(events);
    assert_eq!(metrics.len(), 3);

    // Check the first metric (exception_total)
    assert_eq!(metrics[0].name, "exception_total");
    match &metrics[0].data {
        Some(metric::Data::Sum(sum)) => {
            assert_eq!(sum.is_monotonic, true);
            assert_eq!(
                sum.aggregation_temporality,
                AggregationTemporality::Cumulative as i32
            );
            assert_eq!(sum.data_points.len(), 1);

            match sum.data_points[0].value {
                Some(number_data_point::Value::AsDouble(value)) => {
                    assert_eq!(value, 1.0);
                }
                _ => panic!("Expected AsDouble value"),
            }
        }
        _ => panic!("Expected Sum data"),
    }

    // Check the second metric (bytes_out)
    assert_eq!(metrics[1].name, "bytes_out");
    match &metrics[1].data {
        Some(Data::Sum(sum)) => {
            assert_eq!(sum.is_monotonic, true);
            assert_eq!(
                sum.aggregation_temporality,
                AggregationTemporality::Cumulative as i32
            );
            assert_eq!(sum.data_points.len(), 1);

            match sum.data_points[0].value {
                Some(number_data_point::Value::AsDouble(value)) => {
                    assert_eq!(value, 2.5);
                }
                _ => panic!("Expected AsDouble value"),
            }

            // Check timestamp
            assert_eq!(sum.data_points[0].time_unix_nano, 1542182950123456789);
        }
        _ => panic!("Expected Sum data"),
    }

    // Check the third metric (healthcheck with tags)
    assert_eq!(metrics[2].name, "healthcheck");
    match &metrics[2].data {
        Some(Data::Sum(sum)) => {
            assert_eq!(sum.is_monotonic, true);
            assert_eq!(
                sum.aggregation_temporality,
                AggregationTemporality::Cumulative as i32
            );
            assert_eq!(sum.data_points.len(), 1);

            match sum.data_points[0].value {
                Some(number_data_point::Value::AsDouble(value)) => {
                    assert_eq!(value, 1.0);
                }
                _ => panic!("Expected AsDouble value"),
            }

            // Check timestamp
            assert_eq!(sum.data_points[0].time_unix_nano, 1542182950123456789);

            // Check attributes
            assert_eq!(sum.data_points[0].attributes.len(), 1);
            assert_eq!(sum.data_points[0].attributes[0].key, "region");
            match &sum.data_points[0].attributes[0].value {
                Some(value) => match &value.value {
                    Some(any_value::Value::StringValue(s)) => {
                        assert_eq!(s, "local");
                    }
                    _ => panic!("Expected StringValue"),
                },
                None => panic!("Expected Some value"),
            }
        }
        _ => panic!("Expected Sum data"),
    }
}

#[tokio::test]
async fn encode_events_absolute_gauge() {
    let events = vec![Metric::new(
        "temperature",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 10.0 },
    )];

    let metrics = svc().await.encode_events(events);
    assert_eq!(metrics.len(), 1);

    // Check the gauge metric
    assert_eq!(metrics[0].name, "temperature");
    match &metrics[0].data {
        Some(metric::Data::Gauge(gauge)) => {
            assert_eq!(gauge.data_points.len(), 1);

            match gauge.data_points[0].value {
                Some(number_data_point::Value::AsDouble(value)) => {
                    assert_eq!(value, 10.0);
                }
                _ => panic!("Expected AsDouble value"),
            }
        }
        _ => panic!("Expected Gauge data"),
    }
}

#[tokio::test]
async fn encode_events_distribution() {
    let events = vec![Metric::new(
        "latency",
        MetricKind::Incremental,
        MetricValue::Distribution {
            samples: vector_lib::samples![11.0 => 100, 12.0 => 50],
            statistic: StatisticKind::Histogram,
        },
    )];

    let metrics = svc().await.encode_events(events);
    assert_eq!(metrics.len(), 1);

    // Check the histogram metric
    assert_eq!(metrics[0].name, "latency");
    match &metrics[0].data {
        Some(metric::Data::Histogram(histogram)) => {
            assert_eq!(
                histogram.aggregation_temporality,
                AggregationTemporality::Cumulative as i32
            );
            assert_eq!(histogram.data_points.len(), 1);

            let data_point = &histogram.data_points[0];
            assert_eq!(data_point.count, 150);
            assert_eq!(data_point.sum.unwrap(), 11.0 * 100.0 + 12.0 * 50.0);

            // Check buckets
            assert_eq!(data_point.bucket_counts, vec![100, 50, 0]);
            assert_eq!(data_point.explicit_bounds, vec![11.0, 12.0]);
        }
        _ => panic!("Expected Histogram data"),
    }
}

#[tokio::test]
async fn encode_events_set() {
    let events = vec![Metric::new(
        "users",
        MetricKind::Incremental,
        MetricValue::Set {
            values: vec!["alice".into(), "bob".into()].into_iter().collect(),
        },
    )];

    let metrics = svc().await.encode_events(events);
    assert_eq!(metrics.len(), 1);

    // Check the set metric (converted to sum)
    assert_eq!(metrics[0].name, "users");
    match &metrics[0].data {
        Some(metric::Data::Sum(sum)) => {
            assert_eq!(sum.is_monotonic, false);
            assert_eq!(
                sum.aggregation_temporality,
                AggregationTemporality::Cumulative as i32
            );
            assert_eq!(sum.data_points.len(), 1);

            match sum.data_points[0].value {
                Some(number_data_point::Value::AsDouble(value)) => {
                    assert_eq!(value, 2.0); // Two unique values
                }
                _ => panic!("Expected AsDouble value"),
            }
        }
        _ => panic!("Expected Sum data"),
    }
}

#[test]
fn test_aggregation_temporality_config() {
    // Test default
    let cfg = AggregationTemporalityConfig::default();
    assert!(matches!(cfg, AggregationTemporalityConfig::Cumulative));

    let mut service = OpentelemetryMetricsSvc {
        config: OpentelemetryMetricsSinkConfig {
            endpoint: "http://localhost:4317".to_string(),
            healthcheck_endpoint: "http://localhost:13133".to_string(),
            default_namespace: Some("vector".into()),
            aggregation_temporality: AggregationTemporalityConfig::Delta,
            compression: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            tls: None,
            encoding: Default::default(),
            acknowledgements: Default::default(),
        },
        client: HttpClient::new(None, &ProxyConfig::from_env()).unwrap(),
    };

    let events = vec![Metric::new(
        "counter",
        MetricKind::Incremental,
        MetricValue::Counter { value: 1.0 },
    )];

    let metrics = service.encode_events(events);
    match &metrics[0].data {
        Some(metric::Data::Sum(sum)) => {
            assert_eq!(
                sum.aggregation_temporality,
                AggregationTemporality::Delta as i32
            );
        }
        _ => panic!("Expected Sum data"),
    }
}
