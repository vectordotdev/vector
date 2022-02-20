use std::{collections::BTreeMap, sync::Arc};

use bytes::Bytes;
use chrono::{TimeZone, Utc};
use futures::{channel::mpsc::Receiver, stream, StreamExt};
use hyper::StatusCode;
use indoc::indoc;
use ordered_float::NotNan;
use prost::Message;
use vector_core::event::{BatchNotifier, BatchStatus, Event};

use crate::{
    config::SinkConfig,
    event::{TraceEvent, Value},
    sinks::{
        datadog::traces::DatadogTracesConfig,
        util::test::{build_test_server_status, load_sink},
    },
    test_util::{map_event_batch_stream, next_addr},
};

mod dd_proto {
    include!(concat!(env!("OUT_DIR"), "/dd_trace.rs"));
}

/// Submit traces to a dummy http server
async fn start_test(
    batch_status: BatchStatus,
    http_status_code: StatusCode,
    events: Vec<Event>,
) -> Receiver<(http::request::Parts, Bytes)> {
    let addr = next_addr();
    let config = format!(
        indoc! {r#"
            default_api_key = "atoken"
            compression = "none"
            endpoint = "http://{}"
        "#},
        addr
    );
    let (config, cx) = load_sink::<DatadogTracesConfig>(&config).unwrap();
    let (sink, _) = config.build(cx).await.unwrap();

    let (rx, _trigger, server) = build_test_server_status(addr, http_status_code);
    tokio::spawn(server);

    let (batch, receiver) = BatchNotifier::new_with_receiver();
    let stream = map_event_batch_stream(stream::iter(events), Some(batch));

    let _ = sink.run(stream).await.unwrap();
    assert_eq!(receiver.await, batch_status);

    rx
}

fn simple_span() -> BTreeMap<String, Value> {
    BTreeMap::<String, Value>::from([
        ("service".to_string(), Value::from("a_service")),
        ("name".to_string(), Value::from("a_name")),
        ("resource".to_string(), Value::from("a_resource")),
        ("type".to_string(), Value::from("a_type")),
        ("trace_id".to_string(), Value::Integer(123)),
        ("span_id".to_string(), Value::Integer(456)),
        ("parent_id".to_string(), Value::Integer(789)),
        (
            "start".to_string(),
            Value::from(Utc.timestamp_nanos(1_431_648_000_000_001i64)),
        ),
        ("duration".to_string(), Value::Integer(1000)),
        ("error".to_string(), Value::Integer(404)),
        (
            "meta".to_string(),
            Value::Object(BTreeMap::<String, Value>::from([
                ("foo".to_string(), Value::from("bar")),
                ("bar".to_string(), Value::from("baz")),
            ])),
        ),
        (
            "metrics".to_string(),
            Value::Object(BTreeMap::<String, Value>::from([(
                "a_metric".to_string(),
                Value::Float(NotNan::new(0.577).unwrap()),
            )])),
        ),
    ])
}

fn simple_trace_event() -> TraceEvent {
    let mut t = TraceEvent::default();
    t.insert("language", "a_language");
    t.insert("host", "a_host");
    t.insert("env", "an_env");
    t.insert("trace_id", Value::Integer(123));
    t.insert("spans", Value::Array(vec![Value::from(simple_span())]));
    t.insert(
        "start_time".to_string(),
        Value::from(Utc.timestamp_nanos(1_431_648_000_000_002i64)),
    );
    t.insert(
        "end_time".to_string(),
        Value::from(Utc.timestamp_nanos(1_431_648_000_000_003i64)),
    );
    t
}

fn validate_simple_span(span: dd_proto::Span) {
    assert_eq!(span.service, "a_service");
    assert_eq!(span.name, "a_name");
    assert_eq!(span.resource, "a_resource");
    assert_eq!(span.trace_id, 123);
    assert_eq!(span.span_id, 456);
    assert_eq!(span.parent_id, 789);
    assert_eq!(span.start, 1_431_648_000_000_001);
    assert_eq!(span.duration, 1000);
    assert_eq!(span.error, 404);
    assert_eq!(span.r#type, "a_type");
    assert_eq!(span.meta["foo"], "bar");
    assert_eq!(span.meta["bar"], "baz");
    assert_eq!(span.metrics["a_metric"], 0.577);
}

#[tokio::test]
async fn smoke() {
    let mut t = simple_trace_event();
    t.metadata_mut()
        .set_datadog_api_key(Some(Arc::from("a_key")));

    let mut tr = TraceEvent::from(simple_span());
    tr.insert("host", "a_host");
    tr.insert("env", "an_env");
    tr.insert("language", "a_language");
    tr.metadata_mut()
        .set_datadog_api_key(Some(Arc::from("a_key")));

    let events = vec![Event::Trace(t), Event::Trace(tr)];
    let rx = start_test(BatchStatus::Delivered, StatusCode::OK, events).await;

    // We only take 1 elements as the trace & the APM transaction shall be
    // encoded & emitted in the same payload
    let output = rx.take(1).collect::<Vec<_>>().await.pop();
    assert!(output.is_some());

    let (parts, body) = output.unwrap();
    assert_eq!(
        parts.headers.get("Content-Type").unwrap(),
        "application/x-protobuf"
    );
    assert_eq!(parts.headers.get("DD-API-KEY").unwrap(), "a_key");
    assert_eq!(
        parts.headers.get("X-Datadog-Reported-Languages").unwrap(),
        "a_language"
    );

    let mut decoded_payload = dd_proto::TracePayload::decode(body).unwrap();
    assert_eq!(decoded_payload.traces.len(), 1);
    assert_eq!(decoded_payload.transactions.len(), 1);
    assert_eq!(decoded_payload.host_name, "a_host");
    assert_eq!(decoded_payload.env, "an_env");
    let mut trace = decoded_payload.traces.pop().unwrap();
    assert_eq!(trace.start_time, 1_431_648_000_000_002);
    assert_eq!(trace.end_time, 1_431_648_000_000_003);
    assert_eq!(trace.trace_id, 123);
    validate_simple_span(trace.spans.pop().unwrap());
    validate_simple_span(decoded_payload.transactions.pop().unwrap());
}
