use crate::sinks::datadog::logs::DatadogLogsConfig;
use crate::{
    config::SinkConfig,
    sinks::util::test::{build_test_server_status, load_sink},
    test_util::{next_addr, random_lines_with_stream},
};
use bytes::Bytes;
use futures::{
    channel::mpsc::{Receiver, TryRecvError},
    stream, StreamExt,
};
use http::request::Parts;
use hyper::StatusCode;
use indoc::indoc;
use pretty_assertions::assert_eq;
use std::sync::Arc;
use vector_core::event::Event;
use vector_core::event::{BatchNotifier, BatchStatus};

fn event_with_api_key(msg: &str, key: &str) -> Event {
    let mut e = Event::from(msg);
    e.as_mut_log()
        .metadata_mut()
        .set_datadog_api_key(Some(Arc::from(key)));
    e
}

/// Starts a test sink with random lines running into it
///
/// This function starts a Datadog Logs sink with a simplistic configuration and
/// runs random lines through it, returning a vector of the random lines and a
/// Receiver populated with the result of the sink's operation.
///
/// Testers may set `http_status` and `batch_status`. The first controls what
/// status code faked HTTP responses will have, the second acts as a check on
/// the `Receiver`'s status before being returned to the caller.
async fn start_test(
    http_status: StatusCode,
    batch_status: BatchStatus,
) -> (Vec<String>, Receiver<(http::request::Parts, Bytes)>) {
    let config = indoc! {r#"
            default_api_key = "atoken"
            compression = "none"
        "#};
    let (mut config, cx) = load_sink::<DatadogLogsConfig>(config).unwrap();

    let addr = next_addr();
    // Swap out the endpoint so we can force send it
    // to our local server
    let endpoint = format!("http://{}", addr);
    config.endpoint = Some(endpoint.clone());

    let (sink, _) = config.build(cx).await.unwrap();

    let (rx, _trigger, server) = build_test_server_status(addr, http_status);
    tokio::spawn(server);

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let (expected, events) = random_lines_with_stream(100, 10, Some(batch));

    let _ = sink.run(events).await.unwrap();
    // It's possible that the internal machinery of the sink is still
    // spinning up. We pause here to give the batch time to wind
    // through. Waiting is preferable to adding synchronization into the
    // actual sync code for the sole benefit of these tests.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    assert_eq!(receiver.try_recv(), Ok(batch_status));

    (expected, rx)
}

#[tokio::test]
/// Assert the basic functionality of the sink in good conditions
///
/// This test rigs the sink to return StatusCode::OK to responses, checks that
/// all batches were delivered and then asserts that every message is able to be
/// deserialized.
async fn smoke() {
    let (expected, rx) = start_test(StatusCode::OK, BatchStatus::Delivered).await;

    let output = rx.take(expected.len()).collect::<Vec<_>>().await;

    for (i, val) in output.iter().enumerate() {
        assert_eq!(
            val.0.headers.get("Content-Type").unwrap(),
            "application/json"
        );

        let mut json = serde_json::Deserializer::from_slice(&val.1[..])
            .into_iter::<serde_json::Value>()
            .map(|v| v.expect("decoding json"));

        let json = json.next().unwrap();
        let message = json
            .get(0)
            .unwrap()
            .get("message")
            .unwrap()
            .as_str()
            .unwrap();
        assert_eq!(message, expected[i]);
    }
}

#[tokio::test]
/// Assert delivery error behavior
///
/// In the event that delivery fails -- in this case becaues it is FORBIDDEN --
/// there should be no outbound messages from the sink. That is, receiving from
/// its Receiver must fail.
async fn handles_failure() {
    let (_expected, mut rx) = start_test(StatusCode::FORBIDDEN, BatchStatus::Errored).await;
    let res = rx.try_next();

    assert!(matches!(res, Err(TryRecvError { .. })));
}

#[tokio::test]
/// Assert that metadata API keys are passed correctly
///
/// Datadog sink payloads come with an associated API key. This key can be set
/// per-event or set in the message for an entire payload. This test asserts
/// that, for successful transmission, the API key set in metadata is
/// propagated.
async fn api_key_in_metadata() {
    let (mut config, cx) = load_sink::<DatadogLogsConfig>(indoc! {r#"
            default_api_key = "atoken"
            compression = "none"
        "#})
    .unwrap();

    let addr = next_addr();
    // Swap out the endpoint so we can force send it to our local server
    let endpoint = format!("http://{}", addr);
    config.endpoint = Some(endpoint.clone());

    let (sink, _) = config.build(cx).await.unwrap();

    let (rx, _trigger, server) = build_test_server_status(addr, StatusCode::OK);
    tokio::spawn(server);

    let (expected_messages, events) = random_lines_with_stream(100, 10, None);

    let api_key = "0xDECAFBAD";
    let events = events.map(|mut e| {
        println!("EVENT: {:?}", e);
        e.as_mut_log()
            .metadata_mut()
            .set_datadog_api_key(Some(Arc::from(api_key)));
        e
    });

    let () = sink.run(events).await.unwrap();
    // The log API takes payloads in units of 1,000 and, as a result, we ship in
    // units of 1,000. There will only be a single response on the stream.
    let output: (Parts, Bytes) = rx.take(1).collect::<Vec<_>>().await.pop().unwrap();
    // Check that the header et al are shaped as expected.
    let parts = output.0;
    assert_eq!(parts.headers.get("DD-API-KEY").unwrap(), api_key);
    assert_eq!(
        parts.headers.get("Content-Type").unwrap(),
        "application/json"
    );
    // Check that the body appropriately transmits the messages fed in.
    let body = output.1;
    let payload_array: Vec<serde_json::Value> = serde_json::Deserializer::from_slice(&body[..])
        .into_iter::<serde_json::Value>()
        .map(|v| v.expect("decoding json"))
        .next()
        .unwrap()
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(payload_array.len(), expected_messages.len());
    for (i, obj) in payload_array.into_iter().enumerate() {
        let obj = obj.as_object().unwrap();
        let message = obj.get("message").unwrap().as_str().unwrap();
        assert_eq!(message, expected_messages[i]);
    }
}

#[tokio::test]
/// Assert that events with explicit keys have those keys preserved
///
/// Datadog sink payloads come with an associated API key. This key can be set
/// per-event or set in the message for an entire payload. This test asserts
/// that, for successful transmission, per-event API keys are propagated
/// correctly.
async fn multiple_api_keys() {
    let (mut config, cx) = load_sink::<DatadogLogsConfig>(indoc! {r#"
            default_api_key = "atoken"
            compression = "none"
        "#})
    .unwrap();

    let addr = next_addr();
    // Swap out the endpoint so we can force send it
    // to our local server
    let endpoint = format!("http://{}", addr);
    config.endpoint = Some(endpoint.clone());

    let (sink, _) = config.build(cx).await.unwrap();

    let (rx, _trigger, server) = build_test_server_status(addr, StatusCode::OK);
    tokio::spawn(server);

    let events = vec![
        event_with_api_key("mow", "pkc"),
        event_with_api_key("pnh", "vvo"),
        Event::from("no API key in metadata"),
    ];

    let _ = sink.run(stream::iter(events)).await.unwrap();

    let mut keys = rx
        .take(3)
        .map(|r| r.0.headers.get("DD-API-KEY").unwrap().clone())
        .collect::<Vec<_>>()
        .await;

    keys.sort();
    assert_eq!(keys, vec!["atoken", "pkc", "vvo"])
}
