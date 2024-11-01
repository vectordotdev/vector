use chrono::Utc;
use futures::{future::ready, stream};
use serde::Deserialize;
use vector_lib::event::{Metric, MetricKind, MetricValue};

use super::config::StackdriverConfig;
use crate::{
    config::SinkContext,
    gcp::GcpAuthConfig,
    sinks::{prelude::*, util::test::build_test_server},
    test_util::{
        components::{run_and_assert_sink_compliance, SINK_TAGS},
        http::{always_200_response, spawn_blackhole_http_server},
        next_addr,
    },
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<StackdriverConfig>();
}

#[tokio::test]
async fn component_spec_compliance() {
    let mock_endpoint = spawn_blackhole_http_server(always_200_response).await;

    let config = StackdriverConfig::generate_config().to_string();
    let mut config = StackdriverConfig::deserialize(toml::de::ValueDeserializer::new(&config))
        .expect("config should be valid");

    // If we don't override the credentials path/API key, it tries to directly call out to the Google Instance
    // Metadata API, which we clearly don't have in unit tests. :)
    config.auth.credentials_path = None;
    config.auth.api_key = Some("fake".to_string().into());
    config.endpoint = mock_endpoint.to_string();

    let context = SinkContext::default();
    let (sink, _healthcheck) = config.build(context).await.unwrap();

    let event = Event::Metric(Metric::new(
        "gauge-test",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 1_f64 },
    ));
    run_and_assert_sink_compliance(sink, stream::once(ready(event)), &SINK_TAGS).await;
}

#[tokio::test]
async fn sends_metric() {
    let in_addr = next_addr();
    let config = StackdriverConfig {
        endpoint: format!("http://{in_addr}"),
        auth: GcpAuthConfig {
            api_key: None,
            credentials_path: None,
            skip_authentication: true,
        },
        ..Default::default()
    };
    let (rx, trigger, server) = build_test_server(in_addr);
    tokio::spawn(server);

    let context = SinkContext::default();
    let (sink, _healthcheck) = config.build(context).await.unwrap();
    let timestamp = Utc::now();

    let event = Event::Metric(
        Metric::new(
            "gauge-test",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1_f64 },
        )
        .with_timestamp(Some(timestamp)),
    );
    run_and_assert_sink_compliance(sink, stream::once(ready(event)), &SINK_TAGS).await;

    drop(trigger);

    let event = rx
        .take(1)
        .map(|(_, bytes)| serde_json::from_slice::<serde_json::Value>(&bytes).unwrap())
        .collect::<Vec<_>>()
        .await;

    assert_eq!(
        serde_json::json! ({
        "timeSeries":[{
            "metric":{
                "type":"custom.googleapis.com//metrics/gauge-test",
                "labels":{}
            },
            "resource":{"type":"","labels":{}},
            "metricKind":"GAUGE",
            "valueType":"INT64",
            "points":[{
                "interval":{"endTime":timestamp.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)},
                "value":{"int64Value":"1"}
            }]
        }]
        }),
        event[0]
    );
}

#[tokio::test]
async fn sends_multiple_metrics() {
    let in_addr = next_addr();
    let mut batch = BatchConfig::default();
    batch.max_events = Some(5);

    let config = StackdriverConfig {
        endpoint: format!("http://{in_addr}"),
        auth: GcpAuthConfig {
            api_key: None,
            credentials_path: None,
            skip_authentication: true,
        },
        batch,
        ..Default::default()
    };
    let (rx, trigger, server) = build_test_server(in_addr);
    tokio::spawn(server);

    let context = SinkContext::default();
    let (sink, _healthcheck) = config.build(context).await.unwrap();

    let timestamp1 = Utc::now();
    let timestamp2 = Utc::now();

    let event = vec![
        Event::Metric(
            Metric::new(
                "gauge1",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 1_f64 },
            )
            .with_timestamp(Some(timestamp1)),
        ),
        Event::Metric(
            Metric::new(
                "gauge2",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 5_f64 },
            )
            .with_timestamp(Some(timestamp2)),
        ),
    ];
    run_and_assert_sink_compliance(sink, stream::iter(event), &SINK_TAGS).await;

    drop(trigger);

    let event = rx
        .take(1)
        .map(|(_, bytes)| serde_json::from_slice::<serde_json::Value>(&bytes).unwrap())
        .collect::<Vec<_>>()
        .await;

    assert_eq!(
        serde_json::json! ({
        "timeSeries":[
            {
                "metric":{
                    "type":"custom.googleapis.com//metrics/gauge1",
                    "labels":{}
                },
                "resource":{"type":"","labels":{}},
                "metricKind":"GAUGE",
                "valueType":"INT64",
                "points":[{
                    "interval":{"endTime":timestamp1.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)},
                    "value":{"int64Value":"1"}
                }]
            },
            {
                "metric":{
                    "type":"custom.googleapis.com//metrics/gauge2",
                    "labels":{}
                },
                "resource":{"type":"","labels":{}},
                "metricKind":"GAUGE",
                "valueType":"INT64",
                "points":[{
                    "interval":{"endTime":timestamp2.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)},
                    "value":{"int64Value":"5"}
                }]
            }]
        }),
        event[0]
    );
}

#[tokio::test]
async fn does_not_aggregate_metrics() {
    let in_addr = next_addr();
    let mut batch = BatchConfig::default();
    batch.max_events = Some(5);

    let config = StackdriverConfig {
        endpoint: format!("http://{in_addr}"),
        auth: GcpAuthConfig {
            api_key: None,
            credentials_path: None,
            skip_authentication: true,
        },
        batch,
        ..Default::default()
    };
    let (rx, trigger, server) = build_test_server(in_addr);
    tokio::spawn(server);

    let context = SinkContext::default();
    let (sink, _healthcheck) = config.build(context).await.unwrap();

    let timestamp1 = Utc::now();
    let timestamp2 = Utc::now();

    let event = vec![
        Event::Metric(
            Metric::new(
                "gauge",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 1_f64 },
            )
            .with_timestamp(Some(timestamp1)),
        ),
        Event::Metric(
            Metric::new(
                "gauge",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 5_f64 },
            )
            .with_timestamp(Some(timestamp2)),
        ),
    ];
    run_and_assert_sink_compliance(sink, stream::iter(event), &SINK_TAGS).await;

    drop(trigger);

    let event = rx
        .take(1)
        .map(|(_, bytes)| serde_json::from_slice::<serde_json::Value>(&bytes).unwrap())
        .collect::<Vec<_>>()
        .await;

    assert_eq!(
        serde_json::json! ({
        "timeSeries":[
            {
                "metric":{
                    "type":"custom.googleapis.com//metrics/gauge",
                    "labels":{}
                },
                "resource":{"type":"","labels":{}},
                "metricKind":"GAUGE",
                "valueType":"INT64",
                "points":[{
                    "interval":{"endTime":timestamp1.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)},
                    "value":{"int64Value":"1"}
                }]
            },            {
                "metric":{
                    "type":"custom.googleapis.com//metrics/gauge",
                    "labels":{}
                },
                "resource":{"type":"","labels":{}},
                "metricKind":"GAUGE",
                "valueType":"INT64",
                "points":[{
                    "interval":{"endTime":timestamp2.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)},
                    "value":{"int64Value":"5"}
                }]
            }]
        }),
        event[0]
    );
}
