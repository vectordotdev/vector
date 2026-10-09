//! End-to-end tests for the `victoriametrics` sink against a local HTTP server.

use std::{collections::BTreeMap, io::Read};

use bytes::{Buf, Bytes};
use futures::StreamExt;
use http::{HeaderMap, StatusCode};
use prost::Message;
use vector_common::decompression::CappedDecoder;
use vector_lib::{
    event::{Metric, MetricKind, MetricValue, StatisticKind, metric::Sample},
    metric_tags,
    prometheus::parser::proto,
};

use super::{TOKEN_SECRET_KEY, config::VictoriaMetricsConfig};
use crate::{
    config::{SinkConfig, SinkContext},
    event::Event,
    sinks::util::test::{build_test_server, build_test_server_status},
    test_util::{
        self,
        components::{HTTP_SINK_TAGS, assert_sink_compliance},
    },
};

struct Captured {
    path: String,
    headers: HeaderMap,
    body_len: usize,
    request: proto::WriteRequest,
}

impl Captured {
    /// Flattens the request into `(labels, value)` rows.
    fn rows(&self) -> Vec<(BTreeMap<String, String>, f64)> {
        self.request
            .timeseries
            .iter()
            .map(|series| {
                let labels = series
                    .labels
                    .iter()
                    .map(|label| (label.name.clone(), label.value.clone()))
                    .collect();
                (labels, series.samples[0].value)
            })
            .collect()
    }

    fn names(&self) -> Vec<String> {
        self.rows()
            .into_iter()
            .map(|(labels, _)| labels["__name__"].clone())
            .collect()
    }
}

fn decompress(body: Bytes) -> Vec<u8> {
    let mut decompressed = Vec::new();
    CappedDecoder::zstd(body.reader())
        .expect("body must be zstd-compressed")
        .into_reader()
        .read_to_end(&mut decompressed)
        .expect("body must be zstd-compressed");
    decompressed
}

fn gauge(name: &str, value: f64) -> Metric {
    Metric::new(name, MetricKind::Absolute, MetricValue::Gauge { value })
        .with_tags(Some(metric_tags!("host" => "a")))
}

fn with_token(mut metric: Metric, token: &str) -> Metric {
    metric
        .metadata_mut()
        .secrets_mut()
        .insert(TOKEN_SECRET_KEY, token);
    metric
}

fn events(metrics: impl IntoIterator<Item = Metric>) -> Vec<Event> {
    metrics.into_iter().map(Event::Metric).collect()
}

async fn run(config: &str, events: Vec<Event>) -> Vec<Captured> {
    let (_guard, addr) = test_util::addr::next_addr();
    let (rx, trigger, server) = build_test_server(addr);
    tokio::spawn(server);

    let config: VictoriaMetricsConfig =
        serde_yaml::from_str(&format!("endpoint: http://{addr}\n{config}")).unwrap();
    let (sink, _) = config.build(SinkContext::default()).await.unwrap();
    sink.run_events(events).await.unwrap();
    drop(trigger);

    let mut captured = rx
        .map(|(parts, body)| {
            assert_eq!(parts.method, "POST");
            assert_eq!(parts.headers["content-type"], "application/x-protobuf");
            assert_eq!(parts.headers["content-encoding"], "zstd");
            assert_eq!(parts.headers["x-victoriametrics-remote-write-version"], "1");
            assert!(
                !parts
                    .headers
                    .contains_key("x-prometheus-remote-write-version")
            );

            let body = decompress(body);
            Captured {
                path: parts.uri.path().to_owned(),
                headers: parts.headers,
                body_len: body.len(),
                request: proto::WriteRequest::decode(body.as_slice()).unwrap(),
            }
        })
        .collect::<Vec<_>>()
        .await;
    captured.sort_by(|a, b| a.path.cmp(&b.path));
    captured
}

async fn run_compliant(config: &str, events: Vec<Event>) -> Vec<Captured> {
    assert_sink_compliance(&HTTP_SINK_TAGS, run(config, events)).await
}

#[tokio::test]
async fn writes_to_single_node_path() {
    let captured = run_compliant("", events([gauge("temperature", 21.5)])).await;

    assert_eq!(captured.len(), 1);
    let request = &captured[0];
    assert_eq!(request.path, "/api/v1/write");
    assert!(!request.headers.contains_key("authorization"));
    assert_eq!(request.request.metadata.len(), 1);

    let rows = request.rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0["__name__"], "temperature");
    assert_eq!(rows[0].0["host"], "a");
    assert_eq!(rows[0].1, 21.5);
}

#[tokio::test]
async fn sends_metadata_by_default() {
    let captured = run_compliant("", events([gauge("temperature", 1.0)])).await;

    let metadata = &captured[0].request.metadata;
    assert_eq!(metadata.len(), 1);
    assert_eq!(metadata[0].metric_family_name, "temperature");
    assert_eq!(metadata[0].r#type, proto::MetricType::Gauge as i32);

    let captured = run_compliant("send_metadata: false", events([gauge("temperature", 1.0)])).await;
    assert!(captured[0].request.metadata.is_empty());
}

#[tokio::test]
async fn token_secret_overrides_auth_and_splits_batches() {
    let captured = run_compliant(
        "auth: {strategy: basic, user: vector, password: secret}\nrequest: {headers: {X-Extra: yes}}",
        events([
            with_token(gauge("a", 1.0), "token-a"),
            gauge("b", 2.0),
            with_token(gauge("c", 3.0), "token-a"),
        ]),
    )
    .await;

    assert_eq!(captured.len(), 2);
    let by_auth = captured
        .iter()
        .map(|request| {
            (
                request.headers["authorization"]
                    .to_str()
                    .unwrap()
                    .to_owned(),
                request.names(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        by_auth,
        BTreeMap::from([
            (
                "Basic dmVjdG9yOnNlY3JldA==".to_owned(),
                vec!["b".to_owned()]
            ),
            (
                "Bearer token-a".to_owned(),
                vec!["a".to_owned(), "c".to_owned()]
            ),
        ])
    );
    assert!(
        captured
            .iter()
            .all(|request| request.headers["x-extra"] == "yes")
    );
}

#[tokio::test]
async fn path_mode_splits_batches_by_tenant() {
    let project = |metric: Metric, id: &str| {
        let mut metric = metric;
        metric.replace_tag("project".into(), id.into());
        metric
    };
    let captured = run_compliant(
        "tenant: {mode: path, id: '42:{{ tags.project }}'}",
        events([
            project(gauge("a", 1.0), "1"),
            project(gauge("b", 2.0), "2"),
            project(gauge("c", 3.0), "1"),
        ]),
    )
    .await;

    let paths = captured
        .iter()
        .map(|request| (request.path.as_str(), request.names()))
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        vec![
            (
                "/insert/42:1/prometheus/api/v1/write",
                vec!["a".to_owned(), "c".to_owned()]
            ),
            ("/insert/42:2/prometheus/api/v1/write", vec!["b".to_owned()]),
        ]
    );
}

#[tokio::test]
async fn rejects_metrics_with_invalid_tenant() {
    let mut valid = gauge("valid", 1.0);
    valid.replace_tag("project".into(), "1".into());
    let mut invalid = gauge("invalid", 1.0);
    invalid.replace_tag("project".into(), "../../admin".into());

    let captured = run(
        "tenant: {mode: path, id: '42:{{ tags.project }}'}",
        events([valid, invalid]),
    )
    .await;

    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].path, "/insert/42:1/prometheus/api/v1/write");
    assert_eq!(captured[0].names(), vec!["valid".to_owned()]);
}

#[tokio::test]
async fn labels_mode_adds_tenant_labels() {
    let captured = run_compliant(
        "tenant: {mode: labels, id: '7:3'}",
        events([gauge("a", 1.0)]),
    )
    .await;

    assert_eq!(
        captured[0].path,
        "/insert/multitenant/prometheus/api/v1/write"
    );
    let (labels, _) = &captured[0].rows()[0];
    assert_eq!(labels["vm_account_id"], "7");
    assert_eq!(labels["vm_project_id"], "3");
}

#[tokio::test]
async fn accumulates_incremental_distributions_as_vmrange() {
    let distribution = |values: &[f64]| {
        Metric::new(
            "latency",
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: values
                    .iter()
                    .map(|value| Sample {
                        value: *value,
                        rate: 1,
                    })
                    .collect(),
                statistic: StatisticKind::Summary,
            },
        )
    };
    let captured = run_compliant(
        "batch: {max_events: 1}",
        events([distribution(&[0.5, 2.0]), distribution(&[0.5])]),
    )
    .await;

    // The second request carries the cumulative histogram.
    let requests = captured
        .iter()
        .map(|request| {
            request
                .rows()
                .into_iter()
                .map(|(labels, value)| {
                    (
                        labels["__name__"].clone(),
                        labels.get("vmrange").cloned(),
                        value,
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 2);
    let last = requests
        .iter()
        .find(|rows| {
            rows.iter()
                .any(|row| row.0 == "latency_count" && row.2 == 3.0)
        })
        .expect("a request with the cumulative histogram");
    assert_eq!(
        last,
        &vec![
            (
                "latency_bucket".to_owned(),
                Some("4.642e-01...5.275e-01".to_owned()),
                2.0
            ),
            (
                "latency_bucket".to_owned(),
                Some("1.896e+00...2.154e+00".to_owned()),
                1.0
            ),
            ("latency_sum".to_owned(), None, 3.0),
            ("latency_count".to_owned(), None, 3.0),
        ]
    );
}

#[tokio::test]
async fn bounds_request_size_by_encoded_bytes() {
    // Each distribution spans most `vmrange` buckets, so it expands to hundreds of series.
    let wide_distribution = |index: usize| {
        let mut samples = Vec::new();
        let mut value = 1e-9;
        while value < 1e18 {
            samples.push(Sample { value, rate: 1 });
            value *= 1.2;
        }
        Metric::new(
            format!("latency_{index}"),
            MetricKind::Absolute,
            MetricValue::Distribution {
                samples,
                statistic: StatisticKind::Histogram,
            },
        )
    };
    const MAX_BYTES: usize = 200_000;
    let captured = run_compliant(
        &format!("batch: {{max_bytes: {MAX_BYTES}}}"),
        events((0..20).map(wide_distribution)),
    )
    .await;

    assert!(captured.len() > 1, "the batch must be split");
    for request in &captured {
        assert!(
            request.body_len <= MAX_BYTES,
            "request of {} bytes exceeds the limit",
            request.body_len
        );
    }
    let counts = captured
        .iter()
        .flat_map(Captured::names)
        .filter(|name| name.ends_with("_count"))
        .count();
    assert_eq!(counts, 20);
}

async fn healthcheck(status: StatusCode, config: &str) -> (crate::Result<()>, Vec<String>) {
    let (_guard, addr) = test_util::addr::next_addr();
    let (rx, trigger, server) = build_test_server_status(addr, status);
    tokio::spawn(server);

    let config: VictoriaMetricsConfig =
        serde_yaml::from_str(&format!("endpoint: http://{addr}\n{config}")).unwrap();
    let (_, healthcheck) = config.build(SinkContext::default()).await.unwrap();
    let result = healthcheck.await;
    drop(trigger);

    let requests = rx
        .map(|(parts, body)| {
            assert_eq!(parts.method, "POST");
            assert_eq!(parts.headers["content-encoding"], "zstd");
            assert!(decompress(body).is_empty());
            parts.uri.path().to_owned()
        })
        .collect::<Vec<_>>()
        .await;
    (result, requests)
}

#[tokio::test]
async fn healthcheck_writes_empty_request() {
    let (result, requests) = healthcheck(StatusCode::NO_CONTENT, "").await;
    assert!(result.is_ok());
    assert_eq!(requests, vec!["/api/v1/write"]);

    let (result, requests) =
        healthcheck(StatusCode::NO_CONTENT, "tenant: {mode: path, id: '42'}").await;
    assert!(result.is_ok());
    assert_eq!(requests, vec!["/insert/42:0/prometheus/api/v1/write"]);
}

#[tokio::test]
async fn healthcheck_fails_on_unauthorized() {
    let (result, _) = healthcheck(StatusCode::UNAUTHORIZED, "").await;
    assert!(result.unwrap_err().to_string().contains("401"));
}

#[tokio::test]
async fn healthcheck_skipped_for_dynamic_tenant() {
    let (result, requests) = healthcheck(
        StatusCode::UNAUTHORIZED,
        "tenant: {mode: path, id: '42:{{ tags.p }}'}",
    )
    .await;
    assert!(result.is_ok());
    assert!(requests.is_empty());
}
