use std::sync::Arc;

use bytes::Bytes;
use futures::{
    channel::mpsc::{Receiver, TryRecvError},
    stream::Stream,
    StreamExt,
};
use hyper::StatusCode;
use indoc::indoc;
use similar_asserts::assert_eq;
use vector_lib::event::{BatchNotifier, BatchStatus};

use super::*;
use crate::{
    config::SinkConfig,
    event::EventArray,
    sinks::util::test::{build_test_server_status, load_sink},
    test_util::{
        components::{self, COMPONENT_ERROR_TAGS, HTTP_SINK_TAGS},
        next_addr, random_lines_with_stream,
    },
};

fn random_events_with_stream(
    len: usize,
    count: usize,
    batch: Option<BatchNotifier>,
) -> (Vec<String>, impl Stream<Item = EventArray>) {
    let (lines, stream) = random_lines_with_stream(len, count, batch);
    (
        lines,
        stream.map(|mut events| {
            events.iter_logs_mut().for_each(|log| {
                log.insert("title", "All!");
                log.insert("invalid", "Tik");
            });
            events
        }),
    )
}

async fn start_test(
    http_status: StatusCode,
    batch_status: BatchStatus,
) -> (Vec<String>, Receiver<(http::request::Parts, Bytes)>) {
    let config = indoc! {r#"
            default_api_key = "atoken"
        "#};
    let (mut config, cx) = load_sink::<DatadogEventsConfig>(config).unwrap();

    let addr = next_addr();
    // Swap out the endpoint so we can force send it
    // to our local server
    let endpoint = format!("http://{}", addr);
    config.dd_common.endpoint = Some(endpoint.clone());

    let (sink, _) = config.build(cx).await.unwrap();

    let (rx, _trigger, server) = build_test_server_status(addr, http_status);
    tokio::spawn(server);

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let (expected, events) = random_events_with_stream(100, 10, Some(batch));

    if batch_status == BatchStatus::Delivered {
        components::run_and_assert_sink_compliance(sink, events, &HTTP_SINK_TAGS).await;
    } else {
        components::run_and_assert_sink_error(sink, events, &COMPONENT_ERROR_TAGS).await;
    }

    assert_eq!(receiver.try_recv(), Ok(batch_status));

    (expected, rx)
}

#[tokio::test]
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

        // The json we send to Datadog is an array of events.
        // As we have set batch.max_events to 1, each entry will be
        // an array containing a single record.
        let message = json.get("text").unwrap().as_str().unwrap();
        assert_eq!(message, expected[i]);
    }
}

#[tokio::test]
async fn handles_failure() {
    let (_expected, mut rx) = start_test(StatusCode::FORBIDDEN, BatchStatus::Rejected).await;

    assert!(matches!(rx.try_next(), Err(TryRecvError { .. })));
}

#[tokio::test]
async fn api_key_in_metadata() {
    let (mut config, cx) = load_sink::<DatadogEventsConfig>(indoc! {r#"
            default_api_key = "atoken"
        "#})
    .unwrap();

    let addr = next_addr();
    // Swap out the endpoint so we can force send it
    // to our local server
    let endpoint = format!("http://{}", addr);
    config.dd_common.endpoint = Some(endpoint.clone());

    let (sink, _) = config.build(cx).await.unwrap();

    let (rx, _trigger, server) = build_test_server_status(addr, StatusCode::OK);
    tokio::spawn(server);

    let (expected, events) = random_events_with_stream(100, 10, None);

    let events = events.map(|mut events| {
        events.iter_logs_mut().for_each(|log| {
            log.metadata_mut()
                .set_datadog_api_key(Arc::from("from_metadata"));
        });
        events
    });

    components::run_and_assert_sink_compliance(sink, events, &HTTP_SINK_TAGS).await;
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

        let message = json.get("text").unwrap().as_str().unwrap();
        assert_eq!(message, expected[i]);
    }
}

#[tokio::test]
async fn filter_out_fields() {
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

        let message = json.get("text").unwrap().as_str().unwrap();
        assert_eq!(message, expected[i]);
        assert!(json.get("invalid").is_none());
    }
}
