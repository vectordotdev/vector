use std::{collections::BTreeMap, sync::Arc};

use bytes::Bytes;
use chrono::{TimeZone, Utc};
use futures::{channel::mpsc::Receiver, stream, StreamExt};
use hyper::StatusCode;
use indoc::indoc;
use ordered_float::NotNan;
use prost::Message;
use rmp_serde;
use vector_core::event::{BatchNotifier, BatchStatus, Event};

use super::{dd_proto, ddsketch_full, stats::StatsPayload, DatadogTracesConfig};
use crate::{
    config::SinkConfig,
    event::{TraceEvent, Value},
    sinks::util::test::{build_test_server_status, load_sink},
    test_util::{map_event_batch_stream, next_addr},
};

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

fn simple_span(resource: String) -> BTreeMap<String, Value> {
    BTreeMap::<String, Value>::from([
        ("service".to_string(), Value::from("a_service")),
        ("name".to_string(), Value::from("a_name")),
        ("resource".to_string(), Value::from(resource)),
        ("type".to_string(), Value::from("a_type")),
        ("trace_id".to_string(), Value::Integer(123)),
        ("span_id".to_string(), Value::Integer(456)),
        ("parent_id".to_string(), Value::Integer(789)),
        (
            "start".to_string(),
            Value::from(Utc.timestamp_nanos(1_431_648_000_000_001i64)),
        ),
        ("duration".to_string(), Value::Integer(1_000_000)),
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
            Value::Object(BTreeMap::<String, Value>::from([
                (
                    "a_metric".to_string(),
                    Value::Float(NotNan::new(0.577).unwrap()),
                ),
                (
                    "_top_level".to_string(),
                    Value::Float(NotNan::new(1.0).unwrap()),
                ),
            ])),
        ),
    ])
}

pub fn simple_trace_event(resource: String) -> TraceEvent {
    let mut t = TraceEvent::default();
    t.insert("language", "a_language");
    t.insert("agent_version", "1.23456");
    t.insert("host", "a_host");
    t.insert("env", "an_env");
    t.insert("trace_id", Value::Integer(123));
    t.insert("target_tps", Value::Integer(10));
    t.insert("error_tps", Value::Integer(5));
    t.insert(
        "spans",
        Value::Array(vec![Value::from(simple_span(resource))]),
    );
    t
}

fn validate_simple_span(span: dd_proto::Span, resource: String) {
    assert_eq!(span.service, "a_service");
    assert_eq!(span.name, "a_name");
    assert_eq!(span.resource, resource);
    assert_eq!(span.trace_id, 123);
    assert_eq!(span.span_id, 456);
    assert_eq!(span.parent_id, 789);
    assert_eq!(span.start, 1_431_648_000_000_001);
    assert_eq!(span.duration, 1_000_000);
    assert_eq!(span.error, 404);
    assert_eq!(span.r#type, "a_type");
    assert_eq!(span.meta["foo"], "bar");
    assert_eq!(span.meta["bar"], "baz");
    assert_eq!(span.metrics["a_metric"], 0.577);
}

#[tokio::test]
async fn smoke() {
    let mut t = simple_trace_event("a_resource".to_string());
    t.metadata_mut().set_datadog_api_key(Arc::from("a_key"));
    let events = vec![Event::Trace(t)];
    let rx = start_test(BatchStatus::Delivered, StatusCode::OK, events).await;

    // We take 2 elements as the trace & the APM transaction shall be
    // encoded & emitted in the same payload but we also get an APM stats payload
    let mut output = rx.take(2).collect::<Vec<_>>().await;

    let trace = output.pop();
    let stats = output.pop();

    assert!(trace.is_some());
    assert!(stats.is_some());

    let (trace_parts, trace_body) = trace.unwrap();
    assert_eq!(
        trace_parts.headers.get("Content-Type").unwrap(),
        "application/x-protobuf"
    );
    assert_eq!(trace_parts.headers.get("DD-API-KEY").unwrap(), "a_key");

    let mut decoded_payload = dd_proto::TracePayload::decode(trace_body).unwrap();
    assert_eq!(decoded_payload.tracer_payloads.len(), 1);
    assert_eq!(decoded_payload.host_name, "a_host");
    assert_eq!(decoded_payload.env, "an_env");
    let mut tracer_payload = decoded_payload.tracer_payloads.pop().unwrap();
    assert_eq!(tracer_payload.chunks.len(), 1);
    let mut chunk = tracer_payload.chunks.pop().unwrap();
    assert_eq!(chunk.spans.len(), 1);
    validate_simple_span(chunk.spans.pop().unwrap(), "a_resource".to_string());

    let (stats_parts, stats_body) = stats.unwrap();
    assert_eq!(
        stats_parts.headers.get("Content-Type").unwrap(),
        "application/msgpack"
    );
    assert_eq!(stats_parts.headers.get("DD-API-KEY").unwrap(), "a_key");

    let mut sp: StatsPayload = rmp_serde::from_slice(&stats_body).unwrap();
    assert_eq!(sp.agent_hostname, "a_host");
    assert_eq!(sp.agent_env, "an_env");
    assert_eq!(sp.agent_version, "1.23456");
    assert_eq!(sp.stats.len(), 1);
    let mut csp = sp.stats.pop().unwrap();
    assert_eq!(csp.hostname, "a_host");
    assert_eq!(csp.env, "an_env");

    assert_eq!(csp.stats.len(), 1);
    let mut csb = csp.stats.pop().unwrap();

    let cgs = csb.stats.pop().unwrap();
    assert_eq!(cgs.hits, 1);
    assert_eq!(cgs.top_level_hits, 1);
    assert_eq!(cgs.errors, 1);
    assert_eq!(cgs.duration, 1_000_000);
    assert_eq!(cgs.name, "a_name");
    assert_eq!(cgs.resource, "a_resource");
    assert_eq!(cgs.service, "a_service");

    let ok_summary = ddsketch_full::DdSketch::decode(&cgs.ok_summary[..]).unwrap();
    let error_summary = ddsketch_full::DdSketch::decode(&cgs.error_summary[..]).unwrap();

    assert_eq!(ok_summary.mapping.unwrap().interpolation, 0);
    // No value there because the mocked span has an error field > 0
    assert_eq!(ok_summary.zero_count, 0.0);
    assert_eq!(
        ok_summary
            .positive_values
            .as_ref()
            .unwrap()
            .bin_counts
            .len(),
        0
    );
    assert_eq!(
        ok_summary
            .negative_values
            .as_ref()
            .unwrap()
            .bin_counts
            .len(),
        0
    );

    assert_eq!(error_summary.mapping.unwrap().interpolation, 0);
    // We should have a single positive value
    assert_eq!(error_summary.zero_count, 0.0);
    assert_eq!(
        error_summary
            .positive_values
            .as_ref()
            .unwrap()
            .bin_counts
            .len(),
        1
    );
    assert_eq!(
        error_summary
            .negative_values
            .as_ref()
            .unwrap()
            .bin_counts
            .len(),
        0
    );
}

#[tokio::test]
async fn multiple_traces() {
    let mut t1 = simple_trace_event("trace_1".to_string());
    t1.metadata_mut().set_datadog_api_key(Arc::from("a_key"));
    let mut t2 = simple_trace_event("trace_2".to_string());
    t2.metadata_mut().set_datadog_api_key(Arc::from("a_key"));

    let events = vec![Event::Trace(t1), Event::Trace(t2)];
    let rx = start_test(BatchStatus::Delivered, StatusCode::OK, events).await;

    let mut output = rx.take(2).collect::<Vec<_>>().await;

    let trace = output.pop();
    let stats = output.pop();

    assert!(trace.is_some());
    assert!(stats.is_some());

    let (trace_parts, trace_body) = trace.unwrap();
    assert_eq!(
        trace_parts.headers.get("Content-Type").unwrap(),
        "application/x-protobuf"
    );
    assert_eq!(trace_parts.headers.get("DD-API-KEY").unwrap(), "a_key");

    let mut decoded_payload = dd_proto::TracePayload::decode(trace_body).unwrap();
    assert_eq!(decoded_payload.tracer_payloads.len(), 2);
    assert_eq!(decoded_payload.host_name, "a_host");
    assert_eq!(decoded_payload.env, "an_env");

    ["trace_2", "trace_1"].into_iter().for_each(|t| {
        let mut tracer_payload = decoded_payload.tracer_payloads.pop().unwrap();
        assert_eq!(tracer_payload.chunks.len(), 1);
        let mut chunk = tracer_payload.chunks.pop().unwrap();
        assert_eq!(chunk.spans.len(), 1);
        validate_simple_span(chunk.spans.pop().unwrap(), t.to_string());
    });

    let (stats_parts, stats_body) = stats.unwrap();
    assert_eq!(
        stats_parts.headers.get("Content-Type").unwrap(),
        "application/msgpack"
    );
    assert_eq!(stats_parts.headers.get("DD-API-KEY").unwrap(), "a_key");

    let mut sp: StatsPayload = rmp_serde::from_slice(&stats_body).unwrap();
    assert_eq!(sp.agent_hostname, "a_host");
    assert_eq!(sp.agent_env, "an_env");
    assert_eq!(sp.agent_version, "1.23456");
    assert_eq!(sp.stats.len(), 1);

    let mut csp = sp.stats.pop().unwrap();
    assert_eq!(csp.hostname, "a_host");
    assert_eq!(csp.env, "an_env");
    assert_eq!(csp.stats.len(), 1);

    let mut csb = csp.stats.pop().unwrap();
    // Ensure we got separate ClientStatsBucket for different traces
    assert_eq!(csb.stats.len(), 2);

    let cgs_trace_2 = csb.stats.pop().unwrap();
    assert_eq!(cgs_trace_2.hits, 1);
    assert_eq!(cgs_trace_2.top_level_hits, 1);
    assert_eq!(cgs_trace_2.errors, 1);
    assert_eq!(cgs_trace_2.duration, 1_000_000);
    assert_eq!(cgs_trace_2.name, "a_name");
    assert_eq!(cgs_trace_2.resource, "trace_2");
    assert_eq!(cgs_trace_2.service, "a_service");

    let cgs_trace_1 = csb.stats.pop().unwrap();
    assert_eq!(cgs_trace_1.hits, 1);
    assert_eq!(cgs_trace_1.top_level_hits, 1);
    assert_eq!(cgs_trace_1.errors, 1);
    assert_eq!(cgs_trace_1.duration, 1_000_000);
    assert_eq!(cgs_trace_1.name, "a_name");
    assert_eq!(cgs_trace_1.resource, "trace_1");
    assert_eq!(cgs_trace_1.service, "a_service");
}
