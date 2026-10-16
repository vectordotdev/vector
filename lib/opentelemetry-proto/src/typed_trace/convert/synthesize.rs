//! Typed trace values synthesized back onto OTLP protobuf attributes.
//!
//! Reserved bridge keys are written here so encoding and decoding share one attribute shape.

use vector_core::event::typed_trace::{
    AttrMap, AttrValue, DatadogAgentEnvelope, DatadogChunkContext, DatadogSpanContext,
    DatadogTracerContext, DroppedCount, Resource, Scope, Span, SpanEvent, SpanLink, SpanStatus,
    TraceConversionIssue, TraceConversionReporter, TraceId,
};

use super::super::{
    any_value::{attr_map_to_key_values, attributes_to_key_values},
    keys::{
        AGENT_ENV, AGENT_ERROR_TPS, AGENT_HOST_NAME, AGENT_RARE_SAMPLER, AGENT_TAGS,
        AGENT_TARGET_TPS, AGENT_VERSION, DATADOG_AGENT, DATADOG_CHUNK_DROPPED,
        DATADOG_CHUNK_ORIGIN, DATADOG_CHUNK_PRIORITY, DATADOG_CHUNK_TAGS, DATADOG_SPAN_META_STRUCT,
        DATADOG_SPAN_RESOURCE, DATADOG_SPAN_TYPE, DATADOG_TRACER_TAGS, ENVIRONMENT_NAME, HOST_NAME,
        SERVICE_NAME,
    },
    timestamps::{datetime_to_otlp_nanos, reconstruct_end_time},
};
use super::strip_reserved_map;
use crate::proto::{
    common::v1::InstrumentationScope,
    resource::v1::Resource as PbResource,
    trace::v1::{
        Span as PbSpan, Status as PbStatus,
        span::{Event as PbEvent, Link as PbLink},
    },
};

impl PbResource {
    pub(in crate::typed_trace) fn from_typed(
        resource: Resource,
        agent: Option<DatadogAgentEnvelope>,
        tracer: DatadogTracerContext,
        reporter: &mut impl TraceConversionReporter,
    ) -> (Self, String) {
        let mut dropped = resource.dropped_attributes_count;
        let mut map = resource.attributes.into_inner();
        put_typed_slot(
            &mut map,
            &mut dropped,
            SERVICE_NAME,
            resource.service,
            reporter,
        );
        put_typed_slot(
            &mut map,
            &mut dropped,
            ENVIRONMENT_NAME,
            resource.environment,
            reporter,
        );
        put_typed_slot(&mut map, &mut dropped, HOST_NAME, resource.host, reporter);
        strip_reserved_map(&mut map, &mut dropped, reporter);
        synthesize_resource_datadog(&mut map, agent, tracer);

        (
            Self {
                attributes: attr_map_to_key_values(map),
                dropped_attributes_count: dropped.get(),
            },
            resource.schema_url.unwrap_or_default(),
        )
    }
}

impl InstrumentationScope {
    pub(in crate::typed_trace) fn from_typed(scope: Scope) -> (Self, String) {
        (
            Self {
                name: scope.name.unwrap_or_default(),
                version: scope.version.unwrap_or_default(),
                attributes: attributes_to_key_values(scope.attributes),
                dropped_attributes_count: scope.dropped_attributes_count.get(),
            },
            scope.schema_url.unwrap_or_default(),
        )
    }
}

fn put_typed_slot(
    map: &mut AttrMap,
    dropped: &mut DroppedCount,
    key: &str,
    value: Option<String>,
    reporter: &mut impl TraceConversionReporter,
) {
    if let Some(value) = value
        && map.insert(key.into(), AttrValue::String(value)).is_some()
    {
        dropped.increment(1);
        reporter.report(TraceConversionIssue::AttributeCollision { key });
    }
}

fn synthesize_resource_datadog(
    map: &mut AttrMap,
    agent: Option<DatadogAgentEnvelope>,
    tracer: DatadogTracerContext,
) {
    if let Some(agent) = agent {
        map.insert(DATADOG_AGENT.into(), AttrValue::Map(agent_to_map(agent)));
    }
    if !tracer.tags.is_empty() {
        map.insert(
            DATADOG_TRACER_TAGS.into(),
            AttrValue::Map(tracer.tags.into_inner()),
        );
    }
}

fn agent_to_map(agent: DatadogAgentEnvelope) -> AttrMap {
    AttrMap::from([
        (AGENT_HOST_NAME.into(), AttrValue::String(agent.host_name)),
        (AGENT_ENV.into(), AttrValue::String(agent.env)),
        (AGENT_VERSION.into(), AttrValue::String(agent.agent_version)),
        (AGENT_TARGET_TPS.into(), AttrValue::Float(agent.target_tps)),
        (AGENT_ERROR_TPS.into(), AttrValue::Float(agent.error_tps)),
        (
            AGENT_RARE_SAMPLER.into(),
            AttrValue::Bool(agent.rare_sampler_enabled),
        ),
        (AGENT_TAGS.into(), AttrValue::Map(agent.tags.into_inner())),
    ])
}

impl PbSpan {
    pub(in crate::typed_trace) fn from_typed(
        trace_id: TraceId,
        span: Span,
        chunk: Option<&DatadogChunkContext>,
        reporter: &mut impl TraceConversionReporter,
    ) -> Self {
        let mut dropped = span.dropped_attributes_count;
        let mut map = span.attributes.into_inner();
        strip_reserved_map(&mut map, &mut dropped, reporter);
        synthesize_span_datadog(&mut map, span.datadog);
        if let Some(chunk) = chunk {
            synthesize_chunk_datadog(&mut map, chunk);
        }
        let start_time_unix_nano = datetime_to_otlp_nanos(span.start_time, reporter);

        Self {
            trace_id: trace_id.to_bytes().to_vec(),
            span_id: span.span_id.to_bytes().to_vec(),
            trace_state: span.trace_state.into_raw(),
            parent_span_id: span
                .parent_span_id
                .map(|id| id.to_bytes().to_vec())
                .unwrap_or_default(),
            name: span.name,
            kind: span.kind.to_i32(),
            start_time_unix_nano,
            end_time_unix_nano: reconstruct_end_time(start_time_unix_nano, span.duration, reporter),
            attributes: attr_map_to_key_values(map),
            dropped_attributes_count: dropped.get(),
            events: span
                .events
                .into_iter()
                .map(|event| PbEvent::from_typed(event, reporter))
                .collect(),
            dropped_events_count: span.dropped_events_count.get(),
            links: span.links.into_iter().map(PbLink::from_typed).collect(),
            dropped_links_count: span.dropped_links_count.get(),
            status: Some(PbStatus::from_typed(span.status)),
            flags: span.flags.bits(),
        }
    }
}

impl PbEvent {
    fn from_typed(event: SpanEvent, reporter: &mut impl TraceConversionReporter) -> Self {
        Self {
            time_unix_nano: datetime_to_otlp_nanos(event.time, reporter),
            name: event.name,
            attributes: attributes_to_key_values(event.attributes),
            dropped_attributes_count: event.dropped_attributes_count.get(),
        }
    }
}

impl PbLink {
    fn from_typed(link: SpanLink) -> Self {
        Self {
            trace_id: link.trace_id.to_bytes().to_vec(),
            span_id: link.span_id.to_bytes().to_vec(),
            trace_state: link.trace_state.into_raw(),
            attributes: attributes_to_key_values(link.attributes),
            dropped_attributes_count: link.dropped_attributes_count.get(),
            flags: link.flags.bits(),
        }
    }
}

impl PbStatus {
    fn from_typed(status: SpanStatus) -> Self {
        Self {
            code: status.to_i32(),
            message: status.into_message(),
        }
    }
}

/// Returns `true` when `chunk` synthesizes at least one bridge key, so its presence survives
/// OTLP egress. [`synthesize_chunk_datadog`] omits exactly the default-valued fields.
pub(in crate::typed_trace) fn chunk_has_bridge_keys(chunk: &DatadogChunkContext) -> bool {
    *chunk != DatadogChunkContext::default()
}

fn synthesize_span_datadog(map: &mut AttrMap, span: DatadogSpanContext) {
    if let Some(resource) = span.resource_name {
        map.insert(DATADOG_SPAN_RESOURCE.into(), AttrValue::String(resource));
    }
    if let Some(span_type) = span.span_type {
        map.insert(DATADOG_SPAN_TYPE.into(), AttrValue::String(span_type));
    }
    if !span.meta_struct.is_empty() {
        let meta = span
            .meta_struct
            .into_iter()
            .map(|(k, v)| (k, AttrValue::Bytes(v)))
            .collect();
        map.insert(DATADOG_SPAN_META_STRUCT.into(), AttrValue::Map(meta));
    }
}

/// Synthesizes the chunk bridge keys. Every span of a grouping carries its own copy.
fn synthesize_chunk_datadog(map: &mut AttrMap, chunk: &DatadogChunkContext) {
    if let Some(priority) = chunk.priority {
        map.insert(
            DATADOG_CHUNK_PRIORITY.into(),
            AttrValue::Integer(i64::from(priority.to_i32())),
        );
    }
    if let Some(origin) = &chunk.origin {
        map.insert(
            DATADOG_CHUNK_ORIGIN.into(),
            AttrValue::String(origin.clone()),
        );
    }
    if chunk.dropped {
        map.insert(DATADOG_CHUNK_DROPPED.into(), AttrValue::Bool(true));
    }
    if !chunk.tags.is_empty() {
        map.insert(
            DATADOG_CHUNK_TAGS.into(),
            AttrValue::Map(chunk.tags.clone().into_inner()),
        );
    }
}
