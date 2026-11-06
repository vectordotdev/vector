//! Protobuf `ResourceSpans` decoded into typed trace events.
//!
//! Semantic-convention promotion and Datadog bridge lifting happen here, before a typed
//! [`TraceEvent`] exists.

use std::collections::BTreeMap;

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
        DATADOG_CHUNK_ORIGIN, DATADOG_CHUNK_PRIORITY, DATADOG_CHUNK_SAMPLING_MECHANISM,
        DATADOG_CHUNK_TAGS, DATADOG_SPAN_META_STRUCT, DATADOG_SPAN_RESOURCE, DATADOG_SPAN_TYPE,
        DATADOG_TRACER_TAGS, ENVIRONMENT_LEGACY, ENVIRONMENT_NAME, HOST_NAME, SERVICE_NAME,
    },
    timestamps::{duration_from_otlp, otlp_nanos_to_datetime},
};
use super::{malformed_bridge, strip_reserved_map};
use crate::proto::{
    common::v1::{InstrumentationScope, KeyValue},
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

/// One item's attributes while they are decoded, with the dropped count that every duplicate,
/// malformed bridge key, and promotion collision on that item is charged to.
struct AttributeDecoder<'a, R> {
    attributes: Attributes,
    dropped: DroppedCount,
    reporter: &'a mut R,
}

impl<'a, R: TraceConversionReporter> AttributeDecoder<'a, R> {
    fn new(key_values: Vec<KeyValue>, dropped_attributes_count: u32, reporter: &'a mut R) -> Self {
        let mut dropped = DroppedCount::new(dropped_attributes_count);
        let attributes = key_values_to_attributes(key_values, &mut dropped, reporter);
        Self {
            attributes,
            dropped,
            reporter,
        }
    }

    fn finish(self) -> (Attributes, DroppedCount) {
        (self.attributes, self.dropped)
    }

    fn lift_resource_datadog(&mut self) -> DatadogEventContext {
        let mut datadog = DatadogEventContext::default();
        if let Some(map) = self.lift(DATADOG_AGENT, AttrValue::into_map) {
            let agent = self.lift_agent(map);
            datadog.set_agent(Some(agent));
        }
        if let Some(tags) = self.lift(DATADOG_TRACER_TAGS, AttrValue::into_map) {
            datadog.tracer = DatadogTracerContext {
                tags: Attributes::from(tags),
            };
        }
        self.strip_reserved();
        datadog
    }

    fn lift_span_datadog(&mut self) -> (DatadogSpanContext, SpanChunkKeys) {
        let resource_name = self.lift(DATADOG_SPAN_RESOURCE, AttrValue::into_string);
        let span_type = self.lift(DATADOG_SPAN_TYPE, AttrValue::into_string);
        let meta_struct = self
            .lift(DATADOG_SPAN_META_STRUCT, AttrValue::into_map)
            .map(|map| self.lift_meta_struct(map))
            .unwrap_or_default();
        let chunk = SpanChunkKeys {
            priority: self
                .lift(DATADOG_CHUNK_PRIORITY, |value| {
                    value.into_integer().and_then(|v| i32::try_from(v).ok())
                })
                .map(SamplingPriority::from_i32),
            origin: self.lift(DATADOG_CHUNK_ORIGIN, AttrValue::into_string),
            dropped: self.lift(DATADOG_CHUNK_DROPPED, AttrValue::into_bool),
            sampling_mechanism: self.lift(DATADOG_CHUNK_SAMPLING_MECHANISM, |value| {
                value.into_integer().and_then(|v| u32::try_from(v).ok())
            }),
            tags: self
                .lift(DATADOG_CHUNK_TAGS, AttrValue::into_map)
                .map(Attributes::from),
        };
        self.strip_reserved();
        let span = DatadogSpanContext {
            resource_name,
            span_type,
            meta_struct,
        };
        (span, chunk)
    }

    /// Removes the bridge key `key` and parses it, charging a malformed value to this item.
    fn lift<T>(&mut self, key: &str, parse: fn(AttrValue) -> Option<T>) -> Option<T> {
        let value = self.attributes.remove(key)?;
        self.require(key, parse(value))
    }

    /// Charges a `None` parse of the bridge key `key` to this item as malformed.
    fn require<T>(&mut self, key: &str, parsed: Option<T>) -> Option<T> {
        if parsed.is_none() {
            self.malformed(key);
        }
        parsed
    }

    fn lift_agent(&mut self, map: AttrMap) -> DatadogAgentEnvelope {
        let mut envelope = DatadogAgentEnvelope::default();
        for (key, member) in map {
            self.lift_agent_field(&mut envelope, key.as_ref(), member);
        }
        envelope
    }

    fn lift_agent_field(
        &mut self,
        envelope: &mut DatadogAgentEnvelope,
        key: &str,
        member: AttrValue,
    ) {
        match key {
            AGENT_HOST_NAME => {
                if let Some(s) = self.require(DATADOG_AGENT, member.into_string()) {
                    envelope.host_name = s;
                }
            }
            AGENT_ENV => {
                if let Some(s) = self.require(DATADOG_AGENT, member.into_string()) {
                    envelope.env = s;
                }
            }
            AGENT_VERSION => {
                if let Some(s) = self.require(DATADOG_AGENT, member.into_string()) {
                    envelope.agent_version = s;
                }
            }
            AGENT_TARGET_TPS => {
                if let Some(v) = self.require(DATADOG_AGENT, member.into_float()) {
                    envelope.target_tps = v;
                }
            }
            AGENT_ERROR_TPS => {
                if let Some(v) = self.require(DATADOG_AGENT, member.into_float()) {
                    envelope.error_tps = v;
                }
            }
            AGENT_RARE_SAMPLER => {
                if let Some(v) = self.require(DATADOG_AGENT, member.into_bool()) {
                    envelope.rare_sampler_enabled = v;
                }
            }
            AGENT_TAGS => {
                if let Some(tags) = self.require(DATADOG_AGENT, member.into_map()) {
                    envelope.tags = Attributes::from(tags);
                }
            }
            _ => self.malformed(DATADOG_AGENT),
        }
    }

    fn lift_meta_struct(&mut self, map: AttrMap) -> BTreeMap<KeyString, Bytes> {
        map.into_iter()
            .filter_map(|(key, value)| {
                if let AttrValue::Bytes(bytes) = value {
                    Some((key, bytes))
                } else {
                    self.malformed(DATADOG_SPAN_META_STRUCT);
                    None
                }
            })
            .collect()
    }

    fn malformed(&mut self, key: &str) {
        malformed_bridge(&mut self.dropped, key, self.reporter);
    }

    fn strip_reserved(&mut self) {
        strip_reserved_map(
            self.attributes.as_map_mut(),
            &mut self.dropped,
            self.reporter,
        );
    }

    /// Removes and returns a non-empty string at `key`. Any other value stays in place.
    fn promote(&mut self, key: &str) -> Option<String> {
        if matches!(self.attributes.get(key), Some(AttrValue::String(s)) if !s.is_empty())
            && let Some(AttrValue::String(s)) = self.attributes.remove(key)
        {
            Some(s)
        } else {
            None
        }
    }

    /// Promotes the stable environment key, or the deprecated one when the stable value is
    /// not a non-empty string. A promoted stable value drops the deprecated key.
    fn promote_environment(&mut self) -> Option<String> {
        let stable = self.promote(ENVIRONMENT_NAME);
        if stable.is_some() && self.attributes.remove(ENVIRONMENT_LEGACY).is_some() {
            self.dropped.increment(1);
            self.reporter
                .report(TraceConversionIssue::AttributeCollision {
                    key: ENVIRONMENT_LEGACY,
                });
        }
        stable.or_else(|| self.promote(ENVIRONMENT_LEGACY))
    }
}

/// Chunk-scoped bridge values lifted from one span, or merged across a partition's spans.
/// Every field is an `Option`, so the default value means no chunk key was lifted.
#[derive(Debug, Default, PartialEq)]
struct SpanChunkKeys {
    priority: Option<SamplingPriority>,
    origin: Option<String>,
    dropped: Option<bool>,
    sampling_mechanism: Option<u32>,
    tags: Option<Attributes>,
}

impl SpanChunkKeys {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Merges a later span's values; the first value seen for each key wins.
    fn merge(&mut self, keys: Self, reporter: &mut impl TraceConversionReporter) {
        let Self {
            priority,
            origin,
            dropped,
            sampling_mechanism,
            tags,
        } = keys;
        merge_first(
            priority,
            &mut self.priority,
            DATADOG_CHUNK_PRIORITY,
            reporter,
        );
        merge_first(origin, &mut self.origin, DATADOG_CHUNK_ORIGIN, reporter);
        merge_first(dropped, &mut self.dropped, DATADOG_CHUNK_DROPPED, reporter);
        merge_first(
            sampling_mechanism,
            &mut self.sampling_mechanism,
            DATADOG_CHUNK_SAMPLING_MECHANISM,
            reporter,
        );
        merge_first(tags, &mut self.tags, DATADOG_CHUNK_TAGS, reporter);
    }

    fn into_context(self) -> DatadogChunkContext {
        DatadogChunkContext {
            priority: self.priority,
            origin: self.origin,
            dropped: self.dropped.unwrap_or_default(),
            sampling_mechanism: self.sampling_mechanism,
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

impl PbResource {
    fn into_typed(
        self,
        schema_url: String,
        reporter: &mut impl TraceConversionReporter,
    ) -> (Resource, DatadogEventContext) {
        let mut decoder =
            AttributeDecoder::new(self.attributes, self.dropped_attributes_count, reporter);
        let datadog = decoder.lift_resource_datadog();
        let service = decoder.promote(SERVICE_NAME);
        let environment = decoder.promote_environment();
        let host = decoder.promote(HOST_NAME);
        let (attributes, dropped_attributes_count) = decoder.finish();
        let resource = Resource {
            service,
            environment,
            host,
            attributes,
            schema_url: nonempty_string(schema_url),
            dropped_attributes_count,
        };
        (resource, datadog)
    }
}

impl InstrumentationScope {
    fn into_typed(self, schema_url: String, reporter: &mut impl TraceConversionReporter) -> Scope {
        let (attributes, dropped_attributes_count) =
            AttributeDecoder::new(self.attributes, self.dropped_attributes_count, reporter)
                .finish();
        Scope {
            name: nonempty_string(self.name),
            version: nonempty_string(self.version),
            attributes,
            schema_url: nonempty_string(schema_url),
            dropped_attributes_count,
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

        let mut decoder =
            AttributeDecoder::new(self.attributes, self.dropped_attributes_count, reporter);
        let (span_datadog, chunk_keys) = decoder.lift_span_datadog();
        let (attributes, dropped_attributes_count) = decoder.finish();

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
            dropped_attributes_count,
            dropped_events_count: DroppedCount::new(self.dropped_events_count),
            dropped_links_count,
        };
        Some((trace_id, span, chunk_keys))
    }
}

impl PbEvent {
    fn into_typed(self, reporter: &mut impl TraceConversionReporter) -> SpanEvent {
        let (attributes, dropped_attributes_count) =
            AttributeDecoder::new(self.attributes, self.dropped_attributes_count, reporter)
                .finish();
        SpanEvent {
            name: self.name,
            time: otlp_nanos_to_datetime(self.time_unix_nano),
            attributes,
            dropped_attributes_count,
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
        let (attributes, dropped_attributes_count) =
            AttributeDecoder::new(self.attributes, self.dropped_attributes_count, reporter)
                .finish();
        Some(SpanLink {
            trace_id,
            span_id,
            trace_state: TraceState::from_raw(self.trace_state),
            flags: OtlpSpanFlags::from(self.flags),
            attributes,
            dropped_attributes_count,
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
