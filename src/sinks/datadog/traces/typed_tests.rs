use std::sync::Arc;

use bytes::Bytes;
use chrono::{TimeZone, Utc};
use futures::{StreamExt, channel::mpsc::Receiver, stream};
use hyper::StatusCode;
use indoc::indoc;
use ordered_float::NotNan;
use prost::Message;
use vector_lib::event::{BatchNotifier, BatchStatus, Event};
use vrl::event_path;

use super::{DatadogTracesConfig, apm_stats::StatsPayload, dd_proto};
use crate::{
    config::SinkConfig,
    event::{ObjectMap, TraceEvent, Value},
    sinks::util::test::{build_test_server_status, load_sink},
    test_util::{
        addr::next_addr,
        components::{SINK_TAGS, assert_sink_compliance},
        map_event_batch_stream,
    },
};

async fn start_test(events: Vec<Event>) -> Receiver<(http::request::Parts, Bytes)> {
    assert_sink_compliance(&SINK_TAGS, async {
        let (_guard, addr) = next_addr();
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
        let (rx, _trigger, server) = build_test_server_status(addr, StatusCode::OK);
        tokio::spawn(server);
        let (batch, receiver) = BatchNotifier::new_with_receiver();
        let stream = map_event_batch_stream(stream::iter(events), Some(batch));
        sink.run(stream).await.unwrap();
        assert_eq!(receiver.await, BatchStatus::Delivered);
        rx
    })
    .await
}

fn span(service: &str, parent: i64, span_id: i64, error: i64, rate: Option<f64>) -> ObjectMap {
    let mut metrics =
        ObjectMap::from([("_top_level".into(), Value::Float(NotNan::new(1.0).unwrap()))]);
    if let Some(rate) = rate {
        metrics.insert(
            "_sample_rate".into(),
            Value::Float(NotNan::new(rate).unwrap()),
        );
    }
    ObjectMap::from([
        ("service".into(), Value::from(service)),
        ("name".into(), Value::from("a_name")),
        ("resource".into(), Value::from("a_resource")),
        ("type".into(), Value::from("web")),
        ("trace_id".into(), Value::Integer(42)),
        ("span_id".into(), Value::Integer(span_id)),
        ("parent_id".into(), Value::Integer(parent)),
        (
            "start".into(),
            Value::from(Utc.timestamp_nanos(1_431_648_000_000_001i64)),
        ),
        ("duration".into(), Value::Integer(1_000)),
        ("error".into(), Value::Integer(error)),
        ("meta".into(), Value::Object(ObjectMap::new())),
        ("metrics".into(), Value::Object(metrics)),
    ])
}

fn event(spans: Vec<ObjectMap>) -> Event {
    let mut trace = TraceEvent::default();
    trace.insert(event_path!("host"), "a_host");
    trace.insert(event_path!("env"), "an_env");
    trace.insert(event_path!("agent_version"), "1.23456");
    trace.insert(
        event_path!("spans"),
        Value::Array(spans.into_iter().map(Value::from).collect()),
    );
    trace.metadata_mut().set_datadog_api_key(Arc::from("a_key"));
    Event::Trace(trace)
}

#[tokio::test]
async fn numeric_error_normalizes_to_wire_one() {
    let rx = start_test(vec![event(vec![span("api", 0, 7, 404, None)])]).await;
    let mut output = rx.take(2).collect::<Vec<_>>().await;
    let _stats = output.pop().unwrap();
    let (_parts, body) = output.pop().unwrap();
    let payload = dd_proto::AgentPayload::decode(body).unwrap();
    let span = &payload.tracer_payloads[0].chunks[0].spans[0];
    assert_eq!(span.error, 1);
    assert_eq!(span.service, "api");
    assert_eq!(payload.host_name, "a_host");
}

#[tokio::test]
async fn re_coalesced_services_share_the_root_sample_weight() {
    let rx = start_test(vec![event(vec![
        span("api", 0, 1, 0, Some(0.5)),
        span("db", 1, 2, 0, None),
    ])])
    .await;
    let mut output = rx.take(2).collect::<Vec<_>>().await;
    let (_parts, stats_body) = output.pop().unwrap();
    let stats: StatsPayload = rmp_serde::from_slice(&stats_body).unwrap();
    assert_eq!(stats.stats[0].hostname, "a_host");
    let grouped = &stats.stats[0].stats[0].stats;
    assert_eq!(grouped.len(), 2);
    assert!(
        grouped
            .iter()
            .all(|stat| stat.hits == 2 && stat.top_level_hits == 2)
    );
}
