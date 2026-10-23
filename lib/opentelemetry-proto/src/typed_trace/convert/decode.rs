//! Protobuf `ResourceSpans` decoded into typed trace events.
//!
//! Semantic-convention promotion and Datadog bridge lifting happen here, before a typed
//! [`TraceEvent`] exists.

use bytes::Bytes;
use indexmap::IndexMap;
use vector_core::event::{
    EventMetadata,
    typed_trace::{
        AttrMap, AttrValue, Attributes, DatadogAgentEnvelope, DatadogChunkContext,
        DatadogEventContext, DatadogSpanContext, DatadogTracerContext, DroppedCount, IdField,
        InvalidIdError, OtlpSpanFlags, Resource, SamplingPriority, Scope, Span, SpanEvent, SpanId,
        SpanKind, SpanLink, SpanStatus, TraceConversionIssue, TraceConversionReporter, TraceEvent,
        TraceId, TraceState,
    },
};
use vrl::value::KeyString;

use super::super::{
    any_value::key_values_to_attributes,
    keys::{
        AGENT_ENV, AGENT_ERROR_TPS, AGENT_HOST_NAME, AGENT_RARE_SAMPLER, AGENT_TAGS,
        AGENT_TARGET_TPS, AGENT_VERSION, DATADOG_AGENT, DATADOG_CHUNK_DROPPED,
        DATADOG_CHUNK_ORIGIN, DATADOG_CHUNK_PRIORITY, DATADOG_CHUNK_TAGS, DATADOG_SPAN_META_STRUCT,
        DATADOG_SPAN_RESOURCE, DATADOG_SPAN_TYPE, DATADOG_TRACER_TAGS, ENVIRONMENT_LEGACY,
        ENVIRONMENT_NAME, HOST_NAME, SERVICE_NAME,
    },
    timestamps::{duration_from_otlp, otlp_nanos_to_datetime},
};
use super::{malformed_bridge, strip_reserved_map};
use crate::proto::{
    common::v1::InstrumentationScope,
    resource::v1::Resource as PbResource,
    trace::v1::{
        ResourceSpans, ScopeSpans, Span as PbSpan, Status as PbStatus,
        span::{Event as PbEvent, Link as PbLink},
    },
};

type InvalidId = (IdField, InvalidIdError);

impl ResourceSpans {
    /// Converts this OTLP `ResourceSpans` into typed events, partitioned by trace ID.
    #[must_use]
    pub fn into_typed_events(
        self,
        metadata: &EventMetadata,
        reporter: &mut impl TraceConversionReporter,
    ) -> Vec<TraceEvent> {
        let (resource, resource_datadog) = self
            .resource
            .unwrap_or_default()
            .into_typed(self.schema_url, reporter);
        if self.scope_spans.is_empty() {
            reporter.report(TraceConversionIssue::DiscardedGrouping);
        }
        self.scope_spans
            .into_iter()
            .flat_map(|scope_spans| {
                scope_spans.into_typed_events(&resource, &resource_datadog, metadata, reporter)
            })
            .collect()
    }
}

impl ScopeSpans {
    fn into_typed_events(
        self,
        resource: &Resource,
        resource_datadog: &DatadogEventContext,
        metadata: &EventMetadata,
        reporter: &mut impl TraceConversionReporter,
    ) -> Vec<TraceEvent> {
        let scope = self
            .scope
            .unwrap_or_default()
            .into_typed(self.schema_url, reporter);
        let mut partitions: IndexMap<TraceId, Vec<(Span, SpanChunkKeys)>> = IndexMap::new();
        for (trace_id, span, span_chunk) in self
            .spans
            .into_iter()
            .filter_map(|pb_span| pb_span.into_typed(reporter))
        {
            partitions
                .entry(trace_id)
                .or_default()
                .push((span, span_chunk));
        }
        if partitions.is_empty() {
            reporter.report(TraceConversionIssue::DiscardedGrouping);
        }

        partitions
            .into_iter()
            .map(|(trace_id, items)| {
                trace_event_from_partition(
                    trace_id,
                    items,
                    resource,
                    &scope,
                    resource_datadog,
                    metadata,
                    reporter,
                )
            })
            .collect()
    }
}

fn trace_event_from_partition(
    trace_id: TraceId,
    items: Vec<(Span, SpanChunkKeys)>,
    resource: &Resource,
    scope: &Scope,
    resource_datadog: &DatadogEventContext,
    metadata: &EventMetadata,
    reporter: &mut impl TraceConversionReporter,
) -> TraceEvent {
    let mut chunk = SpanChunkKeys::default();
    let spans = items
        .into_iter()
        .map(|(span, keys)| {
            chunk.merge(keys, reporter);
            span
        })
        .collect();
    let mut datadog = resource_datadog.clone();
    if !chunk.is_empty() {
        datadog.set_chunk(Some(chunk.into_context()));
    }
    TraceEvent::from_parts(
        trace_id,
        resource.clone(),
        scope.clone(),
        datadog,
        spans,
        metadata.clone(),
    )
}

fn nonempty_string(value: String) -> Option<String> {
    if value.is_empty() { None } else { Some(value) }
}

fn promote_resource_slots(resource: &mut Resource, reporter: &mut impl TraceConversionReporter) {
    promote_string_slot(
        &mut resource.attributes,
        SERVICE_NAME,
        &mut resource.service,
    );

    let has_stable = resource.attributes.get(ENVIRONMENT_NAME).is_some();
    if has_stable && resource.attributes.remove(ENVIRONMENT_LEGACY).is_some() {
        resource.dropped_attributes_count.increment(1);
        reporter.report(TraceConversionIssue::AttributeCollision {
            key: ENVIRONMENT_LEGACY,
        });
    }
    let environment_key = if has_stable {
        ENVIRONMENT_NAME
    } else {
        ENVIRONMENT_LEGACY
    };
    promote_string_slot(
        &mut resource.attributes,
        environment_key,
        &mut resource.environment,
    );

    promote_string_slot(&mut resource.attributes, HOST_NAME, &mut resource.host);
}

fn promote_string_slot(attributes: &mut Attributes, key: &str, slot: &mut Option<String>) {
    if matches!(attributes.get(key), Some(AttrValue::String(s)) if !s.is_empty())
        && let Some(AttrValue::String(s)) = attributes.remove(key)
    {
        *slot = Some(s);
    }
}

fn lift_resource_datadog(
    attributes: &mut Attributes,
    dropped: &mut DroppedCount,
    datadog: &mut DatadogEventContext,
    reporter: &mut impl TraceConversionReporter,
) {
    if let Some(map) = lift_attr(
        attributes,
        DATADOG_AGENT,
        AttrValue::into_map,
        dropped,
        reporter,
    ) {
        datadog.set_agent(Some(lift_agent(map, dropped, reporter)));
    }
    if let Some(tags) = lift_attr(
        attributes,
        DATADOG_TRACER_TAGS,
        AttrValue::into_map,
        dropped,
        reporter,
    ) {
        datadog.tracer = DatadogTracerContext {
            tags: Attributes::from(tags),
        };
    }
    strip_reserved_map(attributes.as_map_mut(), dropped, reporter);
}

fn lift_span_datadog(
    mut attributes: Attributes,
    mut dropped: DroppedCount,
    reporter: &mut impl TraceConversionReporter,
) -> (Attributes, DroppedCount, DatadogSpanContext, SpanChunkKeys) {
    let mut span = DatadogSpanContext::default();
    let mut chunk = SpanChunkKeys::default();

    if let Some(s) = lift_attr(
        &mut attributes,
        DATADOG_SPAN_RESOURCE,
        AttrValue::into_string,
        &mut dropped,
        reporter,
    ) {
        span.resource_name = Some(s);
    }
    if let Some(s) = lift_attr(
        &mut attributes,
        DATADOG_SPAN_TYPE,
        AttrValue::into_string,
        &mut dropped,
        reporter,
    ) {
        span.span_type = Some(s);
    }
    if let Some(map) = lift_attr(
        &mut attributes,
        DATADOG_SPAN_META_STRUCT,
        AttrValue::into_map,
        &mut dropped,
        reporter,
    ) {
        span.meta_struct = lift_meta_struct(map, &mut dropped, reporter);
    }
    chunk.priority = lift_attr(
        &mut attributes,
        DATADOG_CHUNK_PRIORITY,
        |value| value.into_integer().and_then(|v| i32::try_from(v).ok()),
        &mut dropped,
        reporter,
    )
    .map(SamplingPriority::from_i32);
    chunk.origin = lift_attr(
        &mut attributes,
        DATADOG_CHUNK_ORIGIN,
        AttrValue::into_string,
        &mut dropped,
        reporter,
    );
    chunk.dropped = lift_attr(
        &mut attributes,
        DATADOG_CHUNK_DROPPED,
        AttrValue::into_bool,
        &mut dropped,
        reporter,
    );
    chunk.tags = lift_attr(
        &mut attributes,
        DATADOG_CHUNK_TAGS,
        AttrValue::into_map,
        &mut dropped,
        reporter,
    )
    .map(Attributes::from);
    strip_reserved_map(attributes.as_map_mut(), &mut dropped, reporter);
    (attributes, dropped, span, chunk)
}

fn lift_attr<T>(
    attributes: &mut Attributes,
    key: &str,
    parse: fn(AttrValue) -> Option<T>,
    dropped: &mut DroppedCount,
    reporter: &mut impl TraceConversionReporter,
) -> Option<T> {
    let parsed = parse(attributes.remove(key)?);
    if parsed.is_none() {
        malformed_bridge(dropped, key, reporter);
    }
    parsed
}

/// Chunk-scoped bridge values lifted from one span, or merged across a partition's spans.
#[derive(Debug, Default)]
struct SpanChunkKeys {
    priority: Option<SamplingPriority>,
    origin: Option<String>,
    dropped: Option<bool>,
    tags: Option<Attributes>,
}

impl SpanChunkKeys {
    fn is_empty(&self) -> bool {
        self.priority.is_none()
            && self.origin.is_none()
            && self.dropped.is_none()
            && self.tags.is_none()
    }

    /// Merges a later span's values; the first value seen for each key wins.
    fn merge(&mut self, keys: Self, reporter: &mut impl TraceConversionReporter) {
        merge_first(
            keys.priority,
            &mut self.priority,
            DATADOG_CHUNK_PRIORITY,
            reporter,
        );
        merge_first(
            keys.origin,
            &mut self.origin,
            DATADOG_CHUNK_ORIGIN,
            reporter,
        );
        merge_first(
            keys.dropped,
            &mut self.dropped,
            DATADOG_CHUNK_DROPPED,
            reporter,
        );
        merge_first(keys.tags, &mut self.tags, DATADOG_CHUNK_TAGS, reporter);
    }

    fn into_context(self) -> DatadogChunkContext {
        DatadogChunkContext {
            priority: self.priority,
            origin: self.origin,
            dropped: self.dropped.unwrap_or_default(),
            tags: self.tags.unwrap_or_default(),
        }
    }
}

fn merge_first<T: PartialEq>(
    incoming: Option<T>,
    dest: &mut Option<T>,
    key: &str,
    reporter: &mut impl TraceConversionReporter,
) {
    match (dest, incoming) {
        (Some(existing), Some(incoming)) if *existing != incoming => {
            reporter.report(TraceConversionIssue::ConflictingChunkContext { key });
        }
        (slot @ None, incoming) => *slot = incoming,
        _ => {}
    }
}

fn lift_agent(
    map: AttrMap,
    dropped: &mut DroppedCount,
    reporter: &mut impl TraceConversionReporter,
) -> DatadogAgentEnvelope {
    let mut envelope = DatadogAgentEnvelope::default();
    for (key, member) in map {
        lift_agent_field(&mut envelope, key.as_ref(), member, dropped, reporter);
    }
    envelope
}

fn lift_agent_field(
    envelope: &mut DatadogAgentEnvelope,
    key: &str,
    member: AttrValue,
    dropped: &mut DroppedCount,
    reporter: &mut impl TraceConversionReporter,
) {
    match key {
        AGENT_HOST_NAME => {
            if let Some(s) = require_bridge_attr(AttrValue::into_string(member), dropped, reporter)
            {
                envelope.host_name = s;
            }
        }
        AGENT_ENV => {
            if let Some(s) = require_bridge_attr(AttrValue::into_string(member), dropped, reporter)
            {
                envelope.env = s;
            }
        }
        AGENT_VERSION => {
            if let Some(s) = require_bridge_attr(AttrValue::into_string(member), dropped, reporter)
            {
                envelope.agent_version = s;
            }
        }
        AGENT_TARGET_TPS => {
            if let Some(v) = require_bridge_attr(AttrValue::into_float(member), dropped, reporter) {
                envelope.target_tps = v;
            }
        }
        AGENT_ERROR_TPS => {
            if let Some(v) = require_bridge_attr(AttrValue::into_float(member), dropped, reporter) {
                envelope.error_tps = v;
            }
        }
        AGENT_RARE_SAMPLER => {
            if let Some(v) = require_bridge_attr(AttrValue::into_bool(member), dropped, reporter) {
                envelope.rare_sampler_enabled = v;
            }
        }
        AGENT_TAGS => {
            if let Some(tags) = require_bridge_attr(AttrValue::into_map(member), dropped, reporter)
            {
                envelope.tags = Attributes::from(tags);
            }
        }
        _ => malformed_bridge(dropped, DATADOG_AGENT, reporter),
    }
}

fn require_bridge_attr<T>(
    parsed: Option<T>,
    dropped: &mut DroppedCount,
    reporter: &mut impl TraceConversionReporter,
) -> Option<T> {
    parsed.or_else(|| {
        malformed_bridge(dropped, DATADOG_AGENT, reporter);
        None
    })
}

fn lift_meta_struct(
    map: AttrMap,
    dropped: &mut DroppedCount,
    reporter: &mut impl TraceConversionReporter,
) -> std::collections::BTreeMap<KeyString, Bytes> {
    map.into_iter()
        .filter_map(|(key, value)| {
            if let AttrValue::Bytes(bytes) = value {
                Some((key, bytes))
            } else {
                malformed_bridge(dropped, DATADOG_SPAN_META_STRUCT, reporter);
                None
            }
        })
        .collect()
}

impl PbResource {
    fn into_typed(
        self,
        schema_url: String,
        reporter: &mut impl TraceConversionReporter,
    ) -> (Resource, DatadogEventContext) {
        let mut dropped = DroppedCount::new(self.dropped_attributes_count);
        let mut attributes = key_values_to_attributes(self.attributes, &mut dropped, reporter);
        let mut datadog = DatadogEventContext::default();
        lift_resource_datadog(&mut attributes, &mut dropped, &mut datadog, reporter);

        let mut resource = Resource {
            service: None,
            environment: None,
            host: None,
            attributes,
            schema_url: nonempty_string(schema_url),
            dropped_attributes_count: dropped,
        };
        promote_resource_slots(&mut resource, reporter);
        (resource, datadog)
    }
}

impl InstrumentationScope {
    fn into_typed(self, schema_url: String, reporter: &mut impl TraceConversionReporter) -> Scope {
        let mut dropped = DroppedCount::new(self.dropped_attributes_count);
        let attributes = key_values_to_attributes(self.attributes, &mut dropped, reporter);
        Scope {
            name: nonempty_string(self.name),
            version: nonempty_string(self.version),
            attributes,
            schema_url: nonempty_string(schema_url),
            dropped_attributes_count: dropped,
        }
    }
}

impl PbSpan {
    fn ids(&self) -> Result<(TraceId, SpanId, Option<SpanId>), InvalidId> {
        Ok((
            TraceId::from_slice(&self.trace_id).map_err(|error| (IdField::TraceId, error))?,
            SpanId::from_slice(&self.span_id).map_err(|error| (IdField::SpanId, error))?,
            self.parent_id()
                .map_err(|error| (IdField::ParentSpanId, error))?,
        ))
    }

    /// Reads `parent_span_id`. An empty or all-zero ID means a root span.
    fn parent_id(&self) -> Result<Option<SpanId>, InvalidIdError> {
        let bytes = self.parent_span_id.as_slice();
        if bytes.is_empty() || bytes == [0; 8] {
            Ok(None)
        } else {
            SpanId::from_slice(bytes).map(Some)
        }
    }

    fn into_typed(
        self,
        reporter: &mut impl TraceConversionReporter,
    ) -> Option<(TraceId, Span, SpanChunkKeys)> {
        let (trace_id, span_id, parent_span_id) = self
            .ids()
            .inspect_err(|&(field, error)| {
                reporter.report(TraceConversionIssue::RejectedSpan { field, error });
            })
            .ok()?;

        let mut dropped_attributes = DroppedCount::new(self.dropped_attributes_count);
        let attributes =
            key_values_to_attributes(self.attributes, &mut dropped_attributes, reporter);
        let (attributes, dropped_attributes, span_datadog, chunk_keys) =
            lift_span_datadog(attributes, dropped_attributes, reporter);

        let events = self
            .events
            .into_iter()
            .map(|event| event.into_typed(reporter))
            .collect();
        let mut dropped_links_count = DroppedCount::new(self.dropped_links_count);
        let links = self
            .links
            .into_iter()
            .filter_map(|link| {
                let link = link.into_typed(reporter);
                if link.is_none() {
                    dropped_links_count.increment(1);
                }
                link
            })
            .collect();

        let span = Span {
            span_id,
            parent_span_id,
            trace_state: TraceState::from_raw(self.trace_state),
            flags: OtlpSpanFlags::from(self.flags),
            name: self.name,
            kind: SpanKind::from_i32(self.kind),
            start_time: otlp_nanos_to_datetime(self.start_time_unix_nano),
            duration: duration_from_otlp(
                self.start_time_unix_nano,
                self.end_time_unix_nano,
                reporter,
            ),
            status: self
                .status
                .map_or(SpanStatus::Unset, |status| status.into_typed(reporter)),
            datadog: span_datadog,
            attributes,
            events,
            links,
            dropped_attributes_count: dropped_attributes,
            dropped_events_count: DroppedCount::new(self.dropped_events_count),
            dropped_links_count,
        };
        Some((trace_id, span, chunk_keys))
    }
}

impl PbEvent {
    fn into_typed(self, reporter: &mut impl TraceConversionReporter) -> SpanEvent {
        let mut dropped = DroppedCount::new(self.dropped_attributes_count);
        SpanEvent {
            name: self.name,
            time: otlp_nanos_to_datetime(self.time_unix_nano),
            attributes: key_values_to_attributes(self.attributes, &mut dropped, reporter),
            dropped_attributes_count: dropped,
        }
    }
}

impl PbLink {
    fn ids(&self) -> Result<(TraceId, SpanId), InvalidId> {
        Ok((
            TraceId::from_slice(&self.trace_id).map_err(|error| (IdField::TraceId, error))?,
            SpanId::from_slice(&self.span_id).map_err(|error| (IdField::SpanId, error))?,
        ))
    }

    fn into_typed(self, reporter: &mut impl TraceConversionReporter) -> Option<SpanLink> {
        let (trace_id, span_id) = self
            .ids()
            .inspect_err(|&(field, error)| {
                reporter.report(TraceConversionIssue::DroppedLink { field, error });
            })
            .ok()?;
        let mut dropped = DroppedCount::new(self.dropped_attributes_count);
        Some(SpanLink {
            trace_id,
            span_id,
            trace_state: TraceState::from_raw(self.trace_state),
            flags: OtlpSpanFlags::from(self.flags),
            attributes: key_values_to_attributes(self.attributes, &mut dropped, reporter),
            dropped_attributes_count: dropped,
        })
    }
}

impl PbStatus {
    fn into_typed(self, reporter: &mut impl TraceConversionReporter) -> SpanStatus {
        if matches!(self.code, SpanStatus::UNSET | SpanStatus::OK) && !self.message.is_empty() {
            reporter.report(TraceConversionIssue::NonconformingStatus);
        }
        SpanStatus::from_i32(self.code, self.message)
    }
}
