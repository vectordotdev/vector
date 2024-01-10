use futures::StreamExt;
use indoc::indoc;
use vector_lib::finalization::{BatchNotifier, BatchStatus};

use crate::sinks::datadog::test_utils::{test_server, ApiStatus};
use crate::{
    common::datadog,
    config::{SinkConfig, SinkContext},
    extra_context::ExtraContext,
    sinks::util::test::load_sink_with_context,
    test_util::{
        components::{run_and_assert_sink_compliance, SINK_TAGS},
        next_addr, random_metrics_with_stream,
    },
};

use super::config::DatadogMetricsConfig;

#[tokio::test]
async fn global_options() {
    let config = "";
    let cx = SinkContext {
        extra_context: ExtraContext::single_value(datadog::Options {
            api_key: Some("global-key".to_string().into()),
            ..Default::default()
        }),
        ..SinkContext::default()
    };
    let (mut config, cx) = load_sink_with_context::<DatadogMetricsConfig>(config, cx).unwrap();

    let addr = next_addr();
    // Swap out the endpoint so we can force send it
    // to our local server
    let endpoint = format!("http://{}", addr);
    config.local_dd_common.endpoint = Some(endpoint.clone());

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
    let cx = SinkContext {
        extra_context: ExtraContext::single_value(datadog::Options {
            api_key: Some("global-key".to_string().into()),
            ..Default::default()
        }),
        ..SinkContext::default()
    };
    let (mut config, cx) = load_sink_with_context::<DatadogMetricsConfig>(config, cx).unwrap();

    let addr = next_addr();
    // Swap out the endpoint so we can force send it
    // to our local server
    let endpoint = format!("http://{}", addr);
    config.local_dd_common.endpoint = Some(endpoint.clone());

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
