use crate::sinks::datadog::logs::DatadogLogsConfig;
use crate::{
    config::SinkConfig,
    sinks::util::test::{build_test_server_status, load_sink},
    test_util::{next_addr, random_lines_with_stream},
};
use bytes::Bytes;
use futures::SinkExt;
use futures::{
    channel::mpsc::{Receiver, TryRecvError},
    stream, StreamExt,
};
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

#[tokio::test]
async fn smoke_json() {
    let (expected, output) = smoke_test().await;

    for (i, val) in output.iter().enumerate() {
        assert_eq!(
            val.0.headers.get("Content-Type").unwrap(),
            "application/json"
        );

        let mut json = serde_json::Deserializer::from_slice(&val.1[..])
            .into_iter::<serde_json::Value>()
            .map(|v| v.expect("decoding json"));

        let json = json.next().unwrap();

        // The json we send to Datadog is an array of events.
        // As we have set batch.max_events to 1, each entry will be
        // an array containing a single record.
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

async fn smoke_test() -> (Vec<String>, Vec<(http::request::Parts, Bytes)>) {
    let (expected, rx) = start_test(StatusCode::OK, BatchStatus::Delivered).await;

    let output = rx.take(expected.len()).collect::<Vec<_>>().await;

    (expected, output)
}

#[tokio::test]
async fn handles_failure_json() {
    handles_failure().await;
}

async fn handles_failure() {
    let (_expected, mut rx) = start_test(StatusCode::FORBIDDEN, BatchStatus::Failed).await;

    assert!(matches!(rx.try_next(), Err(TryRecvError { .. })));
}

async fn start_test(
    http_status: StatusCode,
    batch_status: BatchStatus,
) -> (Vec<String>, Receiver<(http::request::Parts, Bytes)>) {
    let config = indoc! {r#"
            default_api_key = "atoken"
            compression = "none"
            batch.max_events = 1
        "#};
    let (mut config, cx) = load_sink::<DatadogLogsConfig>(&config).unwrap();

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

    assert_eq!(receiver.try_recv(), Ok(batch_status));

    (expected, rx)
}

#[tokio::test]
async fn api_key_in_metadata() {
    let (mut config, cx) = load_sink::<DatadogLogsConfig>(indoc! {r#"
            default_api_key = "atoken"
            compression = "none"
            batch.max_events = 1
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

    let (expected, events) = random_lines_with_stream(100, 10, None);

    let mut events = events.map(|mut e| {
        e.as_mut_log()
            .metadata_mut()
            .set_datadog_api_key(Some(Arc::from("from_metadata")));
        Ok(e)
    });

    let _ = sink.into_sink().send_all(&mut events).await.unwrap();
    let output = rx.take(expected.len()).collect::<Vec<_>>().await;

    for (i, val) in output.iter().enumerate() {
        assert_eq!(val.0.headers.get("DD-API-KEY").unwrap(), "from_metadata");

        assert_eq!(
            val.0.headers.get("Content-Type").unwrap(),
            "application/json"
        );

        let mut json = serde_json::Deserializer::from_slice(&val.1[..])
            .into_iter::<serde_json::Value>()
            .map(|v| v.expect("decoding json"));

        let json = json.next().unwrap();

        // The json we send to Datadog is an array of events.
        // As we have set batch.max_events to 1, each entry will be
        // an array containing a single record.
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
async fn multiple_api_keys() {
    let (mut config, cx) = load_sink::<DatadogLogsConfig>(indoc! {r#"
            default_api_key = "atoken"
            compression = "none"
            batch.max_events = 1
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
