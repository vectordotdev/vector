use std::{convert::TryFrom, sync::Arc};

use futures_util::stream;
use serde_json::Value as JsonValue;
use vector_common::btreemap;
use vector_core::event::{BatchNotifier, BatchStatus, Event, MetricValue};

use super::config::HecMetricsSinkConfig;
use crate::{
    config::{SinkConfig, SinkContext},
    event::{Metric, MetricKind},
    sinks::{
        splunk_hec::common::integration_test_helpers::{
            get_token, splunk_api_address, splunk_hec_address,
        },
        util::{BatchConfig, Compression, TowerRequestConfig},
    },
    template::Template,
    test_util::components::{self, HTTP_SINK_TAGS},
};

const USERNAME: &str = "admin";
const PASSWORD: &str = "password";

async fn config() -> HecMetricsSinkConfig {
    let mut batch = BatchConfig::default();
    batch.max_events = Some(10);

    HecMetricsSinkConfig {
        default_namespace: None,
        default_token: get_token().await,
        endpoint: splunk_hec_address(),
        host_key: "host".into(),
        index: None,
        sourcetype: None,
        source: None,
        compression: Compression::None,
        batch,
        request: TowerRequestConfig::default(),
        tls: None,
        acknowledgements: Default::default(),
    }
}

fn get_gauge(batch: Arc<BatchNotifier>) -> Event {
    Metric::new(
        "example-gauge",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 26.28 },
    )
    .with_tags(Some(
        btreemap! {"tag_gauge_test".to_string() => "tag_gauge_value".to_string()},
    ))
    .with_batch_notifier(&batch)
    .into()
}

fn get_counter(batch: Arc<BatchNotifier>) -> Event {
    Metric::new(
        "example-counter",
        MetricKind::Absolute,
        MetricValue::Counter { value: 26.28 },
    )
    .with_tags(Some(
        btreemap! {"tag_counter_test".to_string() => "tag_counter_value".to_string()},
    ))
    .with_batch_notifier(&batch)
    .into()
}

#[tokio::test]
async fn splunk_insert_counter_metric() {
    let cx = SinkContext::new_test();

    let mut config = config().await;
    config.index = Template::try_from("testmetrics".to_string()).ok();
    let (sink, _) = config.build(cx).await.unwrap();

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let event = get_counter(Arc::clone(&batch));
    drop(batch);
    components::run_sink_event(sink, event, &HTTP_SINK_TAGS).await;
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    assert!(
        metric_dimensions_exist(
            "example-counter",
            &["host", "source", "sourcetype", "tag_counter_test"],
        )
        .await
    );
}

#[tokio::test]
async fn splunk_insert_gauge_metric() {
    let cx = SinkContext::new_test();

    let mut config = config().await;
    config.index = Template::try_from("testmetrics".to_string()).ok();
    let (sink, _) = config.build(cx).await.unwrap();

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let event = get_gauge(Arc::clone(&batch));
    drop(batch);
    components::run_sink_event(sink, event, &HTTP_SINK_TAGS).await;
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    assert!(
        metric_dimensions_exist(
            "example-gauge",
            &["host", "source", "sourcetype", "tag_gauge_test"],
        )
        .await
    );
}

#[tokio::test]
async fn splunk_insert_multiple_counter_metrics() {
    let cx = SinkContext::new_test();

    let mut config = config().await;
    config.index = Template::try_from("testmetrics".to_string()).ok();
    let (sink, _) = config.build(cx).await.unwrap();

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let mut events = Vec::new();
    for _ in [..20] {
        events.push(get_counter(Arc::clone(&batch)))
    }
    drop(batch);
    let events = events.into_iter().map(Into::into);
    components::run_sink(sink, stream::iter(events), &HTTP_SINK_TAGS).await;
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    assert!(
        metric_dimensions_exist(
            "example-counter",
            &["host", "source", "sourcetype", "tag_counter_test"],
        )
        .await
    );
}

#[tokio::test]
async fn splunk_insert_multiple_gauge_metrics() {
    let cx = SinkContext::new_test();

    let mut config = config().await;
    config.index = Template::try_from("testmetrics".to_string()).ok();
    let (sink, _) = config.build(cx).await.unwrap();

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let mut events = Vec::new();
    for _ in [..20] {
        events.push(get_gauge(Arc::clone(&batch)))
    }
    drop(batch);
    let events = events.into_iter().map(Into::into);
    components::run_sink(sink, stream::iter(events), &HTTP_SINK_TAGS).await;
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    assert!(
        metric_dimensions_exist(
            "example-gauge",
            &["host", "source", "sourcetype", "tag_gauge_test"],
        )
        .await
    );
}

// It usually takes ~1 second for the metric to show up in search with all dimensions, so poll
// multiple times.
// Note, Splunk automatically addes [host, source, sourcetype] as default metric dimensions
// https://docs.splunk.com/Documentation/SplunkCloud/latest/Metrics/Overview
async fn metric_dimensions_exist(metric_name: &str, expected_dimensions: &[&str]) -> bool {
    for _ in 0..20usize {
        let resp = metric_dimensions(metric_name).await;
        let actual_dimensions = resp
            .iter()
            .map(|d| d["name"].as_str().unwrap())
            .collect::<Vec<_>>();

        if expected_dimensions
            .iter()
            .all(|d| actual_dimensions.contains(d))
        {
            return true;
        }

        // if all dimensions not present, sleep and continue
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    false
}

async fn metric_dimensions(metric_name: &str) -> Vec<JsonValue> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    let res = client
        .get(format!(
            "{}/services/catalog/metricstore/dimensions?output_mode=json&metric_name={}",
            splunk_api_address(),
            metric_name
        ))
        .form(&vec![("filter", "index=*")])
        .basic_auth(USERNAME, Some(PASSWORD))
        .send()
        .await
        .unwrap();

    let json: JsonValue = res.json().await.unwrap();

    json["entry"].as_array().unwrap().clone()
}
