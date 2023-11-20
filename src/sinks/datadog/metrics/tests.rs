use bytes::Bytes;
use futures::StreamExt;
use hyper::StatusCode;
use indoc::indoc;
use vector_lib::finalization::{BatchNotifier, BatchStatus};

use crate::{
    common::datadog,
    config::{SinkConfig, SinkContext},
    extra_context::ExtraContext,
    sinks::util::test::{build_test_server_status, load_sink_with_context},
    test_util::{
        components::{run_and_assert_sink_compliance, SINK_TAGS},
        next_addr, random_metrics_with_stream,
    },
};

use super::config::DatadogMetricsConfig;

// The sink must support v1 and v2 API endpoints which have different codes for
// signaling status. This enum allows us to signal which API endpoint and what
// kind of response we want our test to model without getting into the details
// of exactly what that code is.
#[allow(dead_code)]
enum ApiStatus {
    OKv1,
    OKv2,
    BadRequestv1,
    BadRequestv2,
}

fn test_server(
    addr: std::net::SocketAddr,
    api_status: ApiStatus,
) -> (
    futures::channel::mpsc::Receiver<(http::request::Parts, Bytes)>,
    stream_cancel::Trigger,
    impl std::future::Future<Output = Result<(), ()>>,
) {
    let status = match api_status {
        ApiStatus::OKv1 => StatusCode::OK,
        ApiStatus::OKv2 => StatusCode::ACCEPTED,
        ApiStatus::BadRequestv1 | ApiStatus::BadRequestv2 => StatusCode::BAD_REQUEST,
    };

    // NOTE: we pass `Trigger` out to the caller even though this suite never
    // uses it as it's being dropped cancels the stream machinery here,
    // indicating failures that might not be valid.
    build_test_server_status(addr, status)
}

#[tokio::test]
async fn global_options() {
    let config = "";
    let mut context = anymap::Map::new();
    context.insert(datadog::Options {
        api_key: Some("global-key".to_string().into()),
        ..Default::default()
    });
    let cx = SinkContext {
        extra_context: ExtraContext::new(context),
        ..SinkContext::default()
    };
    let (mut config, cx) = load_sink_with_context::<DatadogMetricsConfig>(config, cx).unwrap();

    let addr = next_addr();
    // Swap out the endpoint so we can force send it
    // to our local server
    let endpoint = format!("http://{}", addr);
    config.dd_common.endpoint = Some(endpoint.clone());

    let (sink, _) = config.build(cx).await.unwrap();

    let (rx, _trigger, server) = test_server(addr, ApiStatus::OKv1);
    tokio::spawn(server);

    let (batch, receiver) = BatchNotifier::new_with_receiver();
    let (_expected, events) = random_metrics_with_stream(10, Some(batch), None);

    run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;

    assert_eq!(receiver.await, BatchStatus::Delivered);

    let keys = rx
        .take(1)
        .map(|r| r.0.headers.get("DD-API-KEY").unwrap().clone())
        .collect::<Vec<_>>()
        .await;

    assert!(keys
        .iter()
        .all(|value| value.to_str().unwrap() == "global-key"));
}

#[tokio::test]
async fn override_global_options() {
    let config = indoc! {r#"
            default_api_key = "local-key"
        "#};

    // Set a global key option, which should be overridden by the option in the component configuration.
    let mut context = anymap::Map::new();
    context.insert(datadog::Options {
        api_key: Some("global-key".to_string().into()),
        ..Default::default()
    });
    let cx = SinkContext {
        extra_context: ExtraContext::new(context),
        ..SinkContext::default()
    };
    let (mut config, cx) = load_sink_with_context::<DatadogMetricsConfig>(config, cx).unwrap();

    let addr = next_addr();
    // Swap out the endpoint so we can force send it
    // to our local server
    let endpoint = format!("http://{}", addr);
    config.dd_common.endpoint = Some(endpoint.clone());

    let (sink, _) = config.build(cx).await.unwrap();

    let (rx, _trigger, server) = test_server(addr, ApiStatus::OKv1);
    tokio::spawn(server);

    let (batch, receiver) = BatchNotifier::new_with_receiver();
    let (_expected, events) = random_metrics_with_stream(10, Some(batch), None);

    run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;

    assert_eq!(receiver.await, BatchStatus::Delivered);

    let keys = rx
        .take(1)
        .map(|r| r.0.headers.get("DD-API-KEY").unwrap().clone())
        .collect::<Vec<_>>()
        .await;

    assert!(keys
        .iter()
        .all(|value| value.to_str().unwrap() == "local-key"));
}
