use bytes::Bytes;
use futures::StreamExt;
use http::HeaderMap;
use indoc::indoc;
use prost::Message;
use vector_lib::metric_tags;
use vector_lib::prometheus::parser::proto;

use super::*;
use crate::{
    config::SinkContext,
    event::{MetricKind, MetricValue},
    sinks::{prometheus::remote_write::config::RemoteWriteConfig, util::test::build_test_server},
    test_util::{
        self,
        components::{assert_sink_compliance, HTTP_SINK_TAGS},
    },
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<RemoteWriteConfig>();
}

macro_rules! labels {
        ( $( $name:expr => $value:expr ),* ) => {
            vec![ $( proto::Label {
                name: $name.to_string(),
                value: $value.to_string()
            }, )* ]
        }
    }

#[tokio::test]
async fn sends_request() {
    let outputs = send_request("", vec![create_event("gauge-2".into(), 32.0)]).await;
    assert_eq!(outputs.len(), 1);
    let (headers, req) = &outputs[0];

    assert!(!headers.contains_key("x-scope-orgid"));

    assert_eq!(req.timeseries.len(), 1);
    assert_eq!(
        req.timeseries[0].labels,
        labels!("__name__" => "gauge-2", "production" => "true", "region" => "us-west-1")
    );
    assert_eq!(req.timeseries[0].samples.len(), 1);
    assert_eq!(req.timeseries[0].samples[0].value, 32.0);
    assert_eq!(req.metadata.len(), 1);
    assert_eq!(req.metadata[0].r#type, proto::MetricType::Gauge as i32);
    assert_eq!(req.metadata[0].metric_family_name, "gauge-2");
}

#[tokio::test]
async fn sends_authenticated_request() {
    let outputs = send_request(
        indoc! {r#"
                tenant_id = "tenant-%Y"
                [auth]
                strategy = "basic"
                user = "user"
                password = "password"
            "#},
        vec![create_event("gauge-2".into(), 32.0)],
    )
    .await;

    assert_eq!(outputs.len(), 1);
    let (_headers, req) = &outputs[0];

    assert_eq!(req.timeseries.len(), 1);
    assert_eq!(
        req.timeseries[0].labels,
        labels!("__name__" => "gauge-2", "production" => "true", "region" => "us-west-1")
    );
    assert_eq!(req.timeseries[0].samples.len(), 1);
    assert_eq!(req.timeseries[0].samples[0].value, 32.0);
    assert_eq!(req.metadata.len(), 1);
    assert_eq!(req.metadata[0].r#type, proto::MetricType::Gauge as i32);
    assert_eq!(req.metadata[0].metric_family_name, "gauge-2");
}

#[cfg(feature = "aws-config")]
#[tokio::test]
async fn sends_authenticated_aws_request() {
    let outputs = send_request(
        indoc! {r#"
                tenant_id = "tenant-%Y"
                [aws]
                region = "foo"
                [auth]
                strategy = "aws"
                access_key_id = "foo"
                secret_access_key = "bar"
            "#},
        vec![create_event("gauge-2".into(), 32.0)],
    )
    .await;

    assert_eq!(outputs.len(), 1);
    let (headers, _req) = &outputs[0];

    let auth = headers["authorization"]
        .to_str()
        .expect("Missing AWS authorization header");
    assert!(auth.starts_with("AWS4-HMAC-SHA256"));
}

#[tokio::test]
async fn sends_x_scope_orgid_header() {
    let outputs = send_request(
        r#"tenant_id = "tenant""#,
        vec![create_event("gauge-3".into(), 12.0)],
    )
    .await;

    assert_eq!(outputs.len(), 1);
    let (headers, _) = &outputs[0];
    assert_eq!(headers["x-scope-orgid"], "tenant");
}

#[tokio::test]
async fn sends_templated_x_scope_orgid_header() {
    let outputs = send_request(
        r#"tenant_id = "tenant-%Y""#,
        vec![create_event("gauge-3".into(), 12.0)],
    )
    .await;

    assert_eq!(outputs.len(), 1);
    let (headers, _) = &outputs[0];
    let orgid = headers["x-scope-orgid"]
        .to_str()
        .expect("Missing x-scope-orgid header");
    assert!(orgid.starts_with("tenant-20"));
    assert_eq!(orgid.len(), 11);
}

#[tokio::test]
async fn retains_state_between_requests() {
    // This sink converts all incremental events to absolute, and
    // should accumulate their totals between batches.
    let outputs = send_request(
        r#"batch.max_events = 1"#,
        vec![
            create_inc_event("counter-1".into(), 12.0),
            create_inc_event("counter-2".into(), 13.0),
            create_inc_event("counter-1".into(), 14.0),
        ],
    )
    .await;

    assert_eq!(outputs.len(), 3);

    let check_output = |index: usize, name: &str, value: f64| {
        let (_, req) = &outputs[index];
        assert_eq!(req.timeseries.len(), 1);
        assert_eq!(req.timeseries[0].labels, labels!("__name__" => name));
        assert_eq!(req.timeseries[0].samples.len(), 1);
        assert_eq!(req.timeseries[0].samples[0].value, value);
    };
    check_output(0, "counter-1", 12.0);
    check_output(1, "counter-2", 13.0);
    check_output(2, "counter-1", 26.0);
}

#[tokio::test]
async fn aggregates_batches() {
    let outputs = send_request(
        r#"batch.max_events = 3"#,
        vec![
            create_inc_event("counter-1".into(), 12.0),
            create_inc_event("counter-1".into(), 14.0),
            create_inc_event("counter-2".into(), 13.0),
            create_inc_event("counter-2".into(), 14.0),
        ],
    )
    .await;

    assert_eq!(outputs.len(), 1);

    let (_, req) = &outputs[0];
    assert_eq!(req.timeseries.len(), 2);
    assert_eq!(req.timeseries[0].labels, labels!("__name__" => "counter-1"));
    assert_eq!(req.timeseries[0].samples.len(), 1);
    assert_eq!(req.timeseries[0].samples[0].value, 26.0);

    assert_eq!(req.timeseries[1].labels, labels!("__name__" => "counter-2"));
    assert_eq!(req.timeseries[1].samples.len(), 1);
    assert_eq!(req.timeseries[1].samples[0].value, 27.0);
}

#[tokio::test]
async fn doesnt_aggregate_batches() {
    let outputs = send_request(
        indoc! {
            r#"
            batch.max_events = 3
            batch.aggregate = false
            "#
        },
        vec![
            create_inc_event("counter-1".into(), 12.0),
            create_inc_event("counter-1".into(), 14.0),
            create_inc_event("counter-2".into(), 13.0),
            create_inc_event("counter-2".into(), 14.0),
        ],
    )
    .await;

    assert_eq!(outputs.len(), 2);

    // The first three metrics are in the first batch.
    let (_, req) = &outputs[0];
    assert_eq!(req.timeseries.len(), 2);
    assert_eq!(req.timeseries[0].labels, labels!("__name__" => "counter-1"));
    assert_eq!(req.timeseries[0].samples.len(), 2);
    assert_eq!(req.timeseries[0].samples[0].value, 12.0);
    assert_eq!(req.timeseries[0].samples[1].value, 26.0);

    assert_eq!(req.timeseries[1].labels, labels!("__name__" => "counter-2"));
    assert_eq!(req.timeseries[1].samples.len(), 1);
    assert_eq!(req.timeseries[1].samples[0].value, 13.0);

    // The last metric is in the last batch.
    let (_, req) = &outputs[1];
    assert_eq!(req.timeseries[0].labels, labels!("__name__" => "counter-2"));
    assert_eq!(req.timeseries[0].samples.len(), 1);
    assert_eq!(req.timeseries[0].samples[0].value, 27.0);
}

async fn send_request(config: &str, events: Vec<Event>) -> Vec<(HeaderMap, proto::WriteRequest)> {
    assert_sink_compliance(&HTTP_SINK_TAGS, async {
        let addr = test_util::next_addr();
        let (rx, trigger, server) = build_test_server(addr);
        tokio::spawn(server);

        let config = format!("endpoint = \"http://{}/write\"\n{}", addr, config);
        let config: RemoteWriteConfig = toml::from_str(&config).unwrap();
        let cx = SinkContext::default();

        let (sink, _) = config.build(cx).await.unwrap();
        sink.run_events(events).await.unwrap();

        drop(trigger);

        rx.map(|(parts, body)| {
            assert_eq!(parts.method, "POST");
            assert_eq!(parts.uri.path(), "/write");
            let headers = parts.headers;
            assert_eq!(headers["x-prometheus-remote-write-version"], "0.1.0");
            assert_eq!(headers["content-encoding"], "snappy");
            assert_eq!(headers["content-type"], "application/x-protobuf");

            if config.auth.is_some() {
                assert!(headers.contains_key("authorization"));
            }

            let decoded = snap::raw::Decoder::new()
                .decompress_vec(&body)
                .expect("Invalid snappy compressed data");
            let request =
                proto::WriteRequest::decode(Bytes::from(decoded)).expect("Invalid protobuf");
            (headers, request)
        })
        .collect::<Vec<_>>()
        .await
    })
    .await
}

pub(super) fn create_event(name: String, value: f64) -> Event {
    Metric::new(name, MetricKind::Absolute, MetricValue::Gauge { value })
        .with_tags(Some(metric_tags!(
            "region" => "us-west-1",
            "production" => "true",
        )))
        .with_timestamp(Some(chrono::Utc::now()))
        .into()
}

fn create_inc_event(name: String, value: f64) -> Event {
    Metric::new(
        name,
        MetricKind::Incremental,
        MetricValue::Counter { value },
    )
    .with_timestamp(Some(chrono::Utc::now()))
    .into()
}
