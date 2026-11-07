//! Typed [`TraceEvent`] -> OTLP `ExportTraceServiceRequest` encoding.

use indexmap::IndexMap;
use vector_core::event::EventFinalizers;
use vector_core::event::typed_trace::{
    DatadogAgentEnvelope, DatadogChunkContext, DatadogTracerContext, Resource, Scope, Span,
    TraceConversionIssue, TraceConversionReporter, TraceEvent, TraceId,
};

use super::convert::chunk_has_bridge_keys;
use crate::proto::{
    collector::trace::v1::ExportTraceServiceRequest,
    common::v1::InstrumentationScope,
    resource::v1::Resource as PbResource,
    trace::v1::{ResourceSpans, ScopeSpans, Span as PbSpan},
};

impl ExportTraceServiceRequest {
    /// Encodes a batch of typed trace events as one OTLP export request.
    ///
    /// Compatible non-empty events are re-coalesced by resource (including resource-level
    /// Datadog bridge state) then by scope and Datadog chunk bridge keys. Empty events become
    /// isolated empty `ScopeSpans` messages. Span order follows event order, then span order
    /// within each event. Grouping is linear in the number of events, regardless of how many
    /// distinct resources or scopes they carry.
    ///
    /// The returned finalizers are every input event's, including clones shared across a
    /// trace-ID partition. Hold them until delivery finishes: dropping them acknowledges
    /// the batch.
    #[must_use]
    pub fn from_events(
        events: impl IntoIterator<Item = TraceEvent>,
        reporter: &mut impl TraceConversionReporter,
    ) -> (Self, EventFinalizers) {
        let mut finalizers = EventFinalizers::default();
        let mut groups: IndexMap<ResourceKey, ResourceGroup> = IndexMap::new();
        for event in events {
            let (trace_id, resource, scope, mut datadog, spans, mut metadata) = event.into_parts();
            finalizers.merge(metadata.take_finalizers());
            let chunk = datadog.take_chunk();
            groups
                .entry(ResourceKey {
                    resource,
                    agent: datadog.take_agent(),
                    tracer: datadog.tracer,
                })
                .or_default()
                .push(trace_id, scope, chunk, spans, reporter);
        }
        (
            Self {
                resource_spans: groups
                    .into_iter()
                    .map(|(key, group)| ResourceSpans::from_group(key, group, reporter))
                    .collect(),
            },
            finalizers,
        )
    }
}

#[derive(Eq, Hash, PartialEq)]
struct ResourceKey {
    resource: Resource,
    agent: Option<DatadogAgentEnvelope>,
    tracer: DatadogTracerContext,
}

#[derive(Default)]
struct ResourceGroup {
    /// First-seen order is emission order. Coalesced keys merge; each empty event has its own
    /// key so it stays a separate `ScopeSpans` at the position it arrived.
    scopes: IndexMap<ScopeGroupKey, Vec<(TraceId, Span)>>,
}

#[derive(Eq, Hash, PartialEq)]
enum ScopeGroupKey {
    Coalesced {
        scope: Scope,
        chunk: Option<DatadogChunkContext>,
    },
    /// `index` is this group's insertion position, so empty events never share a key.
    Empty { scope: Scope, index: usize },
}

impl ResourceGroup {
    /// Adds one event's scope-level parts, whose resource-level state matches this group, to
    /// its scope group.
    fn push(
        &mut self,
        trace_id: TraceId,
        scope: Scope,
        chunk: Option<DatadogChunkContext>,
        spans: Vec<Span>,
        reporter: &mut impl TraceConversionReporter,
    ) {
        if spans.is_empty() {
            reporter.report(TraceConversionIssue::EmptyEvent);
            let index = self.scopes.len();
            self.scopes
                .insert(ScopeGroupKey::Empty { scope, index }, Vec::new());
        } else {
            self.push_spans(trace_id, scope, chunk, spans, reporter);
        }
    }

    /// Adds non-empty `spans` to the scope group they coalesce into.
    fn push_spans(
        &mut self,
        trace_id: TraceId,
        scope: Scope,
        chunk: Option<DatadogChunkContext>,
        spans: Vec<Span>,
        reporter: &mut impl TraceConversionReporter,
    ) {
        // Group by the chunk's bridge projection: a chunk that synthesizes no keys emits the
        // same spans as an absent one.
        let chunk = match chunk {
            Some(chunk) if !chunk_has_bridge_keys(&chunk) => {
                reporter.report(TraceConversionIssue::UnrepresentableChunkContext);
                None
            }
            chunk => chunk,
        };
        self.scopes
            .entry(ScopeGroupKey::Coalesced { scope, chunk })
            .or_default()
            .extend(spans.into_iter().map(|span| (trace_id, span)));
    }

    fn into_scope_spans(self, reporter: &mut impl TraceConversionReporter) -> Vec<ScopeSpans> {
        self.scopes
            .into_iter()
            .map(|(key, spans)| match key {
                ScopeGroupKey::Coalesced { scope, chunk } => {
                    ScopeSpans::from_typed(scope, chunk.as_ref(), spans, reporter)
                }
                ScopeGroupKey::Empty { scope, .. } => {
                    ScopeSpans::from_typed(scope, None, spans, reporter)
                }
            })
            .collect()
    }
}

impl ResourceSpans {
    fn from_group(
        key: ResourceKey,
        group: ResourceGroup,
        reporter: &mut impl TraceConversionReporter,
    ) -> Self {
        let (resource, schema_url) =
            PbResource::from_typed(key.resource, key.agent, key.tracer, reporter);
        Self {
            resource: Some(resource),
            scope_spans: group.into_scope_spans(reporter),
            schema_url,
        }
    }
}

impl ScopeSpans {
    fn from_typed(
        scope: Scope,
        chunk: Option<&DatadogChunkContext>,
        spans: Vec<(TraceId, Span)>,
        reporter: &mut impl TraceConversionReporter,
    ) -> Self {
        let spans = spans
            .into_iter()
            .map(|(trace_id, span)| PbSpan::from_typed(trace_id, span, chunk, reporter))
            .collect();
        let (scope, schema_url) = InstrumentationScope::from_typed(scope);
        Self {
            scope: Some(scope),
            spans,
            schema_url,
        }
    }
}
