//! Integration tests against single-node VictoriaMetrics, a cluster, and `vmauth`.

use std::{collections::BTreeMap, time::Duration};

use chrono::{DateTime, Utc};
use futures::stream;
use serde::Deserialize;
use vector_lib::{
    event::{Metric, MetricKind, MetricValue, StatisticKind, metric::Sample},
    metric_tags,
};

use super::{TOKEN_SECRET_KEY, config::VictoriaMetricsConfig};
use crate::{
    config::{SinkConfig, SinkContext},
    event::Event,
    test_util::{
        components::{HTTP_SINK_TAGS, run_and_assert_sink_compliance},
        random_string, trace_init,
    },
};

fn address(var: &str, default: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| default.to_owned())
}

fn single_node_address() -> String {
    address("VICTORIAMETRICS_ADDRESS", "http://localhost:8428")
}

fn vminsert_address() -> String {
    address("VMINSERT_ADDRESS", "http://localhost:8480")
}

fn vmselect_address() -> String {
    address("VMSELECT_ADDRESS", "http://localhost:8481")
}

fn vmstorage_address() -> String {
    address("VMSTORAGE_ADDRESS", "http://localhost:8482")
}

fn vmauth_address() -> String {
    address("VMAUTH_ADDRESS", "http://localhost:8427")
}

/// A metric name prefix unique to one test run.
fn prefix() -> String {
    format!("vector_it_{}", random_string(8).to_lowercase())
}

#[derive(Debug, Deserialize)]
struct ExportedSeries {
    metric: BTreeMap<String, String>,
    values: Vec<f64>,
}

async fn build_and_send(config: &str, events: Vec<Event>) {
    let config: VictoriaMetricsConfig = serde_yaml::from_str(config).unwrap();
    let (sink, healthcheck) = config.build(SinkContext::default()).await.unwrap();
    healthcheck.await.expect("healthcheck failed");
    run_and_assert_sink_compliance(sink, stream::iter(events), &HTTP_SINK_TAGS).await;
}

/// Makes recently written samples searchable, then exports every series whose name starts with
/// `name_prefix`.
async fn export(storage: &str, select: &str, name_prefix: &str) -> Vec<ExportedSeries> {
    let client = reqwest::Client::new();
    let query = format!("{{__name__=~\"{name_prefix}.*\"}}");

    for _ in 0..20 {
        client
            .get(format!("{storage}/internal/force_flush"))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();

        let body = client
            .get(format!("{select}/api/v1/export"))
            .query(&[("match[]", query.as_str())])
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .text()
            .await
            .unwrap();
        let series = body
            .lines()
            .map(|line| serde_json::from_str::<ExportedSeries>(line).unwrap())
            .collect::<Vec<_>>();
        if !series.is_empty() {
            return series;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Vec::new()
}

fn find<'a>(
    series: &'a [ExportedSeries],
    name: &str,
    label: Option<(&str, &str)>,
) -> &'a ExportedSeries {
    series
        .iter()
        .find(|series| {
            series.metric["__name__"] == name
                && label.is_none_or(|(key, value)| {
                    series.metric.get(key).map(String::as_str) == Some(value)
                })
        })
        .unwrap_or_else(|| panic!("series {name} {label:?} not found in {series:?}"))
}

fn gauge(name: &str, value: f64, timestamp: DateTime<Utc>) -> Metric {
    Metric::new(name, MetricKind::Absolute, MetricValue::Gauge { value })
        .with_tags(Some(metric_tags!("host" => "vector")))
        .with_timestamp(Some(timestamp))
}

#[tokio::test]
async fn single_node() {
    trace_init();
    let prefix = prefix();
    let now = Utc::now();

    let counter = |value: f64, offset_ms: i64| {
        Metric::new(
            format!("{prefix}_requests"),
            MetricKind::Incremental,
            MetricValue::Counter { value },
        )
        .with_timestamp(Some(now + chrono::Duration::milliseconds(offset_ms)))
    };
    let distribution = Metric::new(
        format!("{prefix}_latency"),
        MetricKind::Incremental,
        MetricValue::Distribution {
            samples: vec![
                Sample {
                    value: 0.5,
                    rate: 2,
                },
                Sample {
                    value: 20.0,
                    rate: 1,
                },
            ],
            statistic: StatisticKind::Histogram,
        },
    )
    .with_timestamp(Some(now));

    build_and_send(
        &format!("endpoint: {}", single_node_address()),
        vec![
            gauge(&format!("{prefix}_temperature"), 21.5, now).into(),
            counter(1.0, 0).into(),
            counter(2.0, 1).into(),
            distribution.into(),
        ],
    )
    .await;

    let series = export(&single_node_address(), &single_node_address(), &prefix).await;

    let temperature = find(&series, &format!("{prefix}_temperature"), None);
    assert_eq!(temperature.metric["host"], "vector");
    assert_eq!(temperature.values, vec![21.5]);

    let requests = find(&series, &format!("{prefix}_requests"), None);
    assert_eq!(requests.values, vec![1.0, 3.0]);

    let bucket = find(
        &series,
        &format!("{prefix}_latency_bucket"),
        Some(("vmrange", "4.642e-01...5.275e-01")),
    );
    assert_eq!(bucket.values, vec![2.0]);
    let count = find(&series, &format!("{prefix}_latency_count"), None);
    assert_eq!(count.values, vec![3.0]);
    let sum = find(&series, &format!("{prefix}_latency_sum"), None);
    assert_eq!(sum.values, vec![21.0]);
}

#[tokio::test]
async fn cluster_path_tenant() {
    trace_init();
    let prefix = prefix();

    build_and_send(
        &format!(
            "endpoint: {}\ntenant: {{mode: path, id: '42:5'}}",
            vminsert_address()
        ),
        vec![gauge(&format!("{prefix}_up"), 1.0, Utc::now()).into()],
    )
    .await;

    let select = format!("{}/select/42:5/prometheus", vmselect_address());
    let series = export(&vmstorage_address(), &select, &prefix).await;
    assert_eq!(
        find(&series, &format!("{prefix}_up"), None).values,
        vec![1.0]
    );
}

#[tokio::test]
async fn cluster_label_tenant() {
    trace_init();
    let prefix = prefix();
    let now = Utc::now();
    let mut first = gauge(&format!("{prefix}_up"), 1.0, now);
    first.replace_tag("project".into(), "1".into());
    let mut second = gauge(&format!("{prefix}_up"), 2.0, now);
    second.replace_tag("project".into(), "2".into());

    build_and_send(
        &format!(
            "endpoint: {}\ntenant: {{mode: labels, id: '7:{{{{ tags.project }}}}'}}",
            vminsert_address()
        ),
        vec![first.into(), second.into()],
    )
    .await;

    for (project, value) in [("1", 1.0), ("2", 2.0)] {
        let select = format!("{}/select/7:{project}/prometheus", vmselect_address());
        let series = export(&vmstorage_address(), &select, &prefix).await;
        let up = find(&series, &format!("{prefix}_up"), None);
        assert_eq!(up.values, vec![value]);
        // vminsert removes the tenant labels before storage.
        assert!(!up.metric.contains_key("vm_account_id"));
        assert!(!up.metric.contains_key("vm_project_id"));
    }
}

#[tokio::test]
async fn vmauth_routes_by_token() {
    trace_init();
    let prefix = prefix();
    let now = Utc::now();
    let mut routed = gauge(&format!("{prefix}_up"), 2.0, now);
    routed
        .metadata_mut()
        .secrets_mut()
        .insert(TOKEN_SECRET_KEY, "tenant-2-token");

    build_and_send(
        &format!(
            "endpoint: {}\nauth: {{strategy: bearer, token: tenant-1-token}}",
            vmauth_address()
        ),
        vec![
            gauge(&format!("{prefix}_up"), 1.0, now).into(),
            routed.into(),
        ],
    )
    .await;

    for (tenant, value) in [("1", 1.0), ("2", 2.0)] {
        let select = format!("{}/select/{tenant}/prometheus", vmselect_address());
        let series = export(&vmstorage_address(), &select, &prefix).await;
        assert_eq!(
            find(&series, &format!("{prefix}_up"), None).values,
            vec![value]
        );
    }
}

#[tokio::test]
async fn vmauth_healthcheck_rejects_unknown_token() {
    trace_init();
    let config: VictoriaMetricsConfig = serde_yaml::from_str(&format!(
        "endpoint: {}\nauth: {{strategy: bearer, token: unknown}}",
        vmauth_address()
    ))
    .unwrap();
    let (_, healthcheck) = config.build(SinkContext::default()).await.unwrap();
    let error = healthcheck.await.unwrap_err().to_string();
    assert!(error.contains("401"), "{error}");
}
