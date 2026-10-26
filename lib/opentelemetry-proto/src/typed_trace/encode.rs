//! Typed [`TraceEvent`] -> OTLP `ExportTraceServiceRequest` encoding.

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
    /// within each event.
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
        let mut groups: Vec<ResourceGroup> = Vec::new();
        let mut last_group = None;
        for event in events {
            let (trace_id, resource, scope, mut datadog, spans, mut metadata) = event.into_parts();
            finalizers.merge(metadata.take_finalizers());
            let agent = datadog.take_agent();
            let chunk = datadog.take_chunk();
            let tracer = datadog.tracer;
            let found = recent_match(last_group, &groups, |group| {
                group.matches(&resource, agent.as_ref(), &tracer)
            });
            let index = if let Some(index) = found {
                index
            } else {
                groups.push(ResourceGroup {
                    resource,
                    agent,
                    tracer,
                    scope_groups: Vec::new(),
                    last_scope: None,
                });
                groups.len() - 1
            };
            last_group = Some(index);
            groups[index].push(trace_id, scope, chunk, spans, reporter);
        }
        (
            Self {
                resource_spans: groups
                    .into_iter()
                    .map(|group| ResourceSpans::from_group(group, reporter))
                    .collect(),
            },
            finalizers,
        )
    }
}

struct ResourceGroup {
    resource: Resource,
    agent: Option<DatadogAgentEnvelope>,
    tracer: DatadogTracerContext,
    scope_groups: Vec<ScopeGroup>,
    last_scope: Option<usize>,
}

/// Index of the matching item, checking `last` before scanning.
///
/// Events from one request tend to share a resource and scope with their predecessor, so
/// the repeated full equality scan is the uncommon path.
fn recent_match<T>(
    last: Option<usize>,
    items: &[T],
    matches: impl Fn(&T) -> bool,
) -> Option<usize> {
    last.filter(|&index| matches(&items[index]))
        .or_else(|| items.iter().position(&matches))
}

enum ScopeGroup {
    Coalesced {
        scope: Scope,
        chunk: Option<DatadogChunkContext>,
        spans: Vec<(TraceId, Span)>,
    },
    Empty {
        scope: Scope,
    },
}

impl ScopeGroup {
    fn coalesces(&self, event_scope: &Scope, event_chunk: Option<&DatadogChunkContext>) -> bool {
        matches!(
            self,
            Self::Coalesced { scope, chunk, .. }
                if scope == event_scope && chunk.as_ref() == event_chunk
        )
    }
}

impl ResourceGroup {
    fn matches(
        &self,
        resource: &Resource,
        agent: Option<&DatadogAgentEnvelope>,
        tracer: &DatadogTracerContext,
    ) -> bool {
        self.resource == *resource && self.agent.as_ref() == agent && self.tracer == *tracer
    }

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
            self.scope_groups.push(ScopeGroup::Empty { scope });
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
        let spans = spans.into_iter().map(|span| (trace_id, span));
        if let Some(index) = recent_match(self.last_scope, &self.scope_groups, |group| {
            group.coalesces(&scope, chunk.as_ref())
        }) {
            let ScopeGroup::Coalesced {
                spans: group_spans, ..
            } = &mut self.scope_groups[index]
            else {
                unreachable!("coalesces is false for an empty scope group");
            };
            group_spans.extend(spans);
            self.last_scope = Some(index);
        } else {
            self.scope_groups.push(ScopeGroup::Coalesced {
                scope,
                chunk,
                spans: spans.collect(),
            });
            self.last_scope = Some(self.scope_groups.len() - 1);
        }
    }
}

impl ResourceSpans {
    fn from_group(group: ResourceGroup, reporter: &mut impl TraceConversionReporter) -> Self {
        let (resource, schema_url) =
            PbResource::from_typed(group.resource, group.agent, group.tracer, reporter);
        Self {
            resource: Some(resource),
            scope_spans: group
                .scope_groups
                .into_iter()
                .map(|group| ScopeSpans::from_group(group, reporter))
                .collect(),
            schema_url,
        }
    }
}

impl ScopeSpans {
    fn from_group(group: ScopeGroup, reporter: &mut impl TraceConversionReporter) -> Self {
        let (scope, spans) = match group {
            ScopeGroup::Empty { scope } => (scope, Vec::new()),
            ScopeGroup::Coalesced {
                scope,
                chunk,
                spans,
            } => {
                let spans = spans
                    .into_iter()
                    .map(|(trace_id, span)| {
                        PbSpan::from_typed(trace_id, span, chunk.as_ref(), reporter)
                    })
                    .collect();
                (scope, spans)
            }
        };
        let (scope, schema_url) = InstrumentationScope::from_typed(scope);
        Self {
            scope: Some(scope),
            spans,
            schema_url,
        }
    }
}
