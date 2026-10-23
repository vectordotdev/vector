//! Typed Datadog trace encoding.
//!
//! Converts canonical [`crate::event::typed_trace::TraceEvent`]s into Datadog
//! `AgentPayload`s. The `datadog_traces` sink consumes this after intake conversion;
//! generic HTTP sinks do not.

// The sink migration (the next change) is the production caller. Tests cover the
// encoder until that call exists.
#![allow(dead_code)]

mod derive;
mod grouping;
mod keys;
mod span;
mod string;

#[cfg(test)]
mod testutil;

// The sink migration is the production caller. These names are unused until then.
#[allow(unused_imports)]
pub(crate) use grouping::{AgentGroup, ChunkGroup, TracerGroup, group_typed_traces};
#[cfg_attr(not(test), allow(unused_imports))]
pub(crate) use string::dd_value_to_string;

use super::dd_proto;

#[cfg(test)]
mod tests {
    use super::{AgentGroup, dd_proto, group_typed_traces};
    use crate::{
        event::typed_trace::{
            AttrValue, DatadogAgentEnvelope, DatadogChunkContext, SamplingPriority, SpanKind,
            SpanStatus, TraceId,
        },
        sinks::datadog::traces::typed::testutil::{Issues, event, span},
    };
    use similar_asserts::assert_eq;

    fn derived_span(
        service: &str,
        name: &str,
        resource: &str,
        span_type: &str,
        span_id: u64,
        meta: &[(&str, &str)],
    ) -> dd_proto::Span {
        dd_proto::Span {
            service: service.to_owned(),
            name: name.to_owned(),
            resource: resource.to_owned(),
            trace_id: 1,
            span_id,
            meta: meta
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
            r#type: span_type.to_owned(),
            ..Default::default()
        }
    }

    /// Cross-format cases aligned with current Datadog Agent `main`
    /// (`SpanKind2Type` and `GetOTelResourceV2` in `pkg/trace/transform/otelutil.go`,
    /// read 2026-10-23). These are the committed fixtures for OTLP-derived egress.
    #[test]
    fn current_agent_cross_format_fixture() {
        let mut server = event(1, Some("web"), vec![span(1, "GET /health")]);
        server.spans_mut()[0].kind = SpanKind::Server;
        server.spans_mut()[0]
            .attributes
            .insert("http.request.method", AttrValue::String("GET".into()));
        server.spans_mut()[0]
            .attributes
            .insert("http.route", AttrValue::String("/health".into()));

        let mut client = event(1, Some("db"), vec![span(2, "query")]);
        client.spans_mut()[0].kind = SpanKind::Client;
        client.spans_mut()[0]
            .attributes
            .insert("db.system", AttrValue::String("postgresql".into()));
        client.spans_mut()[0]
            .attributes
            .insert("db.statement", AttrValue::String("select 1".into()));

        let mut cache = event(1, Some("cache"), vec![span(3, "get")]);
        cache.spans_mut()[0].kind = SpanKind::Client;
        cache.spans_mut()[0]
            .attributes
            .insert("db.system", AttrValue::String("redis".into()));

        let mut authoritative = event(
            TraceId::from_datadog(0x11, 0xabc).expect("non-zero").get(),
            Some("checkout"),
            vec![span(4, "ignored")],
        );
        authoritative.spans_mut()[0].kind = SpanKind::Client;
        authoritative.spans_mut()[0]
            .attributes
            .insert("db.system", AttrValue::String("postgres".into()));
        authoritative.spans_mut()[0].datadog.span_type = Some("web".into());
        authoritative.spans_mut()[0].datadog.resource_name = Some("explicit".into());
        authoritative.spans_mut()[0].status = SpanStatus::Error("missing".into());
        authoritative
            .datadog_mut()
            .set_agent(Some(DatadogAgentEnvelope {
                host_name: "agent-host".into(),
                target_tps: f64::NAN,
                ..DatadogAgentEnvelope::default()
            }));
        authoritative
            .datadog_mut()
            .set_chunk(Some(DatadogChunkContext {
                priority: Some(SamplingPriority::AutoReject),
                ..DatadogChunkContext::default()
            }));
        authoritative.resource_mut().host = Some("tracer-host".into());

        let mut issues = Issues::default();
        let groups: Vec<AgentGroup> =
            group_typed_traces(vec![server, client, cache, authoritative], &mut issues);
        assert_eq!(issues.unexpected.len(), 0);

        let derived = groups[0].encode();
        assert_eq!(derived.host_name, "");
        assert_eq!(derived.target_tps.to_bits(), 0.0f64.to_bits());
        assert_eq!(
            derived.tracer_payloads[0].chunks[0],
            dd_proto::TraceChunk {
                priority: 1,
                spans: vec![
                    derived_span(
                        "web",
                        "GET /health",
                        "GET /health",
                        "web",
                        1,
                        &[("http.request.method", "GET"), ("http.route", "/health"),],
                    ),
                    derived_span(
                        "db",
                        "query",
                        "select 1",
                        "db",
                        2,
                        &[("db.statement", "select 1"), ("db.system", "postgresql")],
                    ),
                    derived_span("cache", "get", "get", "cache", 3, &[("db.system", "redis")]),
                ],
                ..Default::default()
            }
        );

        let owned = groups[1].encode();
        assert_eq!(owned.host_name, "agent-host");
        assert!(owned.target_tps.is_nan());
        assert_eq!(owned.tracer_payloads[0].hostname, "tracer-host");
        let span = &owned.tracer_payloads[0].chunks[0].spans[0];
        assert_eq!(span.r#type, "web");
        assert_eq!(span.resource, "explicit");
        assert_eq!(span.error, 1);
        assert_eq!(
            span.meta.get("error.message").map(String::as_str),
            Some("missing")
        );
        assert_eq!(span.trace_id, 0x11);
        assert_eq!(
            span.meta.get("_dd.p.tid").map(String::as_str),
            Some("0000000000000abc")
        );
        assert_eq!(owned.tracer_payloads[0].chunks[0].priority, 0);
        assert!(owned.tracer_payloads[0].container_debug.is_none());
    }

    #[test]
    fn stringification_helper_is_reachable_from_the_module() {
        assert_eq!(
            super::dd_value_to_string(&AttrValue::Float(f64::NAN)),
            "NaN"
        );
    }
}
