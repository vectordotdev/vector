//! Nested `AgentPayload` / `TracerPayload` / `TraceChunk` grouping.
//!
//! Service partitions of one trace re-coalesce. A later event whose service is already
//! in that chunk starts another chunk: ingress emits one event per `(trace_id, service)`,
//! so a repeated service is a distinct source chunk. A new service joins the earliest
//! matching chunk that does not yet contain it.

use std::collections::{BTreeMap, HashSet};

use super::{
    dd_proto, keys, keys::TRACER_ATTRIBUTE_KEYS, span::encode_span, string::dd_value_to_string,
};
use crate::event::typed_trace::{
    AttrValue, Attributes, SamplingPriority, TraceConversionIssue, TraceConversionReporter,
    TraceEvent, TraceId,
};

/// One wire `AgentPayload` and the events that produced it, in input order.
pub(crate) struct AgentGroup {
    key: AgentKey,
    /// `AgentPayload.hostName`. Empty when the event has no agent envelope.
    pub host_name: String,
    /// `AgentPayload.env`.
    pub env: String,
    /// `AgentPayload.agentVersion`.
    pub agent_version: String,
    /// `AgentPayload.targetTPS`, including NaN and signed zero.
    pub target_tps: f64,
    /// `AgentPayload.errorTPS`, including NaN and signed zero.
    pub error_tps: f64,
    /// `AgentPayload.rareSamplerEnabled`.
    pub rare_sampler_enabled: bool,
    /// Normalized agent tags.
    pub tags: BTreeMap<String, String>,
    /// Tracer payloads in first-seen order.
    pub tracers: Vec<TracerGroup>,
    /// Events placed in this payload, in input order.
    pub events: Vec<TraceEvent>,
}

/// One wire `TracerPayload`.
pub(crate) struct TracerGroup {
    key: TracerKey,
    /// `TracerPayload.hostname`, from `Resource.host`.
    pub hostname: String,
    /// `TracerPayload.env`, from `Resource.environment`.
    pub env: String,
    /// `TracerPayload.containerID`.
    pub container_id: String,
    /// `TracerPayload.languageName`.
    pub language_name: String,
    /// `TracerPayload.languageVersion`.
    pub language_version: String,
    /// `TracerPayload.tracerVersion`.
    pub tracer_version: String,
    /// `TracerPayload.runtimeID`.
    pub runtime_id: String,
    /// `TracerPayload.appVersion`.
    pub app_version: String,
    /// Normalized tracer tags.
    pub tags: BTreeMap<String, String>,
    /// Chunks in first-seen order, including one empty chunk per spanless event.
    pub chunks: Vec<ChunkGroup>,
}

/// One wire `TraceChunk`.
pub(crate) struct ChunkGroup {
    key: Option<ChunkKey>,
    services: HashSet<String>,
    /// Trace ID shared by the spans. Also recorded for an empty event, which has no span to carry it.
    pub trace_id: TraceId,
    /// Wire sampling priority. Absent priority is `1` (`AutoKeep`).
    pub priority: i32,
    /// Wire origin. Absent and empty both encode as an empty string; grouping still distinguishes them.
    pub origin: String,
    /// `TraceChunk.droppedTrace`.
    pub dropped: bool,
    /// Normalized chunk tags.
    pub tags: BTreeMap<String, String>,
    /// Indexes into the enclosing [`AgentGroup::events`], in concatenation order.
    pub event_indexes: Vec<usize>,
    /// Wire spans in re-coalesced order.
    pub spans: Vec<dd_proto::Span>,
}

/// Groups `events` into Datadog agent payloads.
///
/// Normalization issues and empty events are reported through `reporter`.
#[must_use]
pub(crate) fn group_typed_traces(
    events: Vec<TraceEvent>,
    reporter: &mut impl TraceConversionReporter,
) -> Vec<AgentGroup> {
    let mut groups: Vec<AgentGroup> = Vec::new();
    for event in events {
        let (key, fields) = agent_parts(&event, reporter);
        let index = groups.iter().position(|group| group.key == key);
        if let Some(index) = index {
            groups[index].push_event(event, reporter);
        } else {
            let mut group = AgentGroup::from_fields(fields, key);
            group.push_event(event, reporter);
            groups.push(group);
        }
    }
    groups
}

impl AgentGroup {
    fn from_fields(fields: AgentFields, key: AgentKey) -> Self {
        Self {
            key,
            host_name: fields.host_name,
            env: fields.env,
            agent_version: fields.agent_version,
            target_tps: fields.target_tps,
            error_tps: fields.error_tps,
            rare_sampler_enabled: fields.rare_sampler_enabled,
            tags: fields.tags,
            tracers: Vec::new(),
            events: Vec::new(),
        }
    }

    fn push_event(&mut self, event: TraceEvent, reporter: &mut impl TraceConversionReporter) {
        let event_index = self.events.len();
        let key = tracer_key(&event, reporter);
        let index = self.tracers.iter().position(|tracer| tracer.key == key);
        if let Some(index) = index {
            self.tracers[index].push_event(&event, event_index, reporter);
        } else {
            let mut tracer = TracerGroup::from_event(&event, key, reporter);
            tracer.push_event(&event, event_index, reporter);
            self.tracers.push(tracer);
        }
        self.events.push(event);
    }

    /// Encodes this group as one `AgentPayload`.
    #[must_use]
    pub(crate) fn encode(&self) -> dd_proto::AgentPayload {
        dd_proto::AgentPayload {
            host_name: self.host_name.clone(),
            env: self.env.clone(),
            tracer_payloads: self.tracers.iter().map(TracerGroup::encode).collect(),
            tags: self.tags.clone(),
            agent_version: self.agent_version.clone(),
            target_tps: self.target_tps,
            error_tps: self.error_tps,
            rare_sampler_enabled: self.rare_sampler_enabled,
            idx_tracer_payloads: Vec::new(),
        }
    }
}

impl TracerGroup {
    fn from_event(
        event: &TraceEvent,
        key: TracerKey,
        reporter: &mut impl TraceConversionReporter,
    ) -> Self {
        let resource = event.resource();
        let attributes = &resource.attributes;
        let tags = key.tags.clone();
        Self {
            hostname: resource.host.clone().unwrap_or_default(),
            env: resource.environment.clone().unwrap_or_default(),
            container_id: project_attr(attributes, keys::CONTAINER_ID, reporter),
            language_name: project_attr(attributes, keys::TELEMETRY_SDK_LANGUAGE, reporter),
            language_version: project_attr(attributes, keys::PROCESS_RUNTIME_VERSION, reporter),
            tracer_version: project_attr(attributes, keys::TELEMETRY_SDK_VERSION, reporter),
            runtime_id: project_attr(attributes, keys::RUNTIME_ID, reporter),
            app_version: project_attr(attributes, keys::SERVICE_VERSION, reporter),
            tags,
            key,
            chunks: Vec::new(),
        }
    }

    fn push_event(
        &mut self,
        event: &TraceEvent,
        event_index: usize,
        reporter: &mut impl TraceConversionReporter,
    ) {
        if event.spans().is_empty() {
            reporter.report(TraceConversionIssue::EmptyEvent);
            self.chunks
                .push(ChunkGroup::empty(event, event_index, reporter));
            return;
        }
        let key = chunk_key(event, reporter);
        let service = event.resource().service.clone().unwrap_or_default();
        if let Some(chunk) = self
            .chunks
            .iter_mut()
            .find(|chunk| chunk.key.as_ref() == Some(&key) && !chunk.services.contains(&service))
        {
            chunk.services.insert(service.clone());
            chunk.event_indexes.push(event_index);
            chunk.extend_spans(event, &service, reporter);
        } else {
            self.chunks.push(ChunkGroup::open(
                event,
                key,
                &service,
                event_index,
                reporter,
            ));
        }
    }

    fn encode(&self) -> dd_proto::TracerPayload {
        dd_proto::TracerPayload {
            container_id: self.container_id.clone(),
            language_name: self.language_name.clone(),
            language_version: self.language_version.clone(),
            tracer_version: self.tracer_version.clone(),
            runtime_id: self.runtime_id.clone(),
            chunks: self.chunks.iter().map(ChunkGroup::encode).collect(),
            tags: self.tags.clone(),
            env: self.env.clone(),
            hostname: self.hostname.clone(),
            app_version: self.app_version.clone(),
            container_debug: None,
        }
    }
}

impl ChunkGroup {
    fn from_key(key: ChunkKey) -> Self {
        let trace_id = key.trace_id;
        let priority = key.priority;
        let origin = key.origin.clone().unwrap_or_default();
        let dropped = key.dropped;
        let tags = key.tags.clone();
        Self {
            key: Some(key),
            services: HashSet::new(),
            trace_id,
            priority,
            origin,
            dropped,
            tags,
            event_indexes: Vec::new(),
            spans: Vec::new(),
        }
    }

    fn open(
        event: &TraceEvent,
        key: ChunkKey,
        service: &str,
        event_index: usize,
        reporter: &mut impl TraceConversionReporter,
    ) -> Self {
        let mut chunk = Self::from_key(key);
        chunk.services.insert(service.to_owned());
        chunk.event_indexes.push(event_index);
        chunk.extend_spans(event, service, reporter);
        chunk
    }

    fn empty(
        event: &TraceEvent,
        event_index: usize,
        reporter: &mut impl TraceConversionReporter,
    ) -> Self {
        let mut chunk = Self::from_key(chunk_key(event, reporter));
        // An empty event does not join a span chunk, and nothing joins it.
        chunk.key = None;
        chunk.event_indexes.push(event_index);
        chunk
    }

    fn extend_spans(
        &mut self,
        event: &TraceEvent,
        service: &str,
        reporter: &mut impl TraceConversionReporter,
    ) {
        for span in event.spans() {
            self.spans.push(encode_span(
                event.trace_id(),
                service,
                event.resource(),
                span,
                reporter,
            ));
        }
    }

    fn encode(&self) -> dd_proto::TraceChunk {
        dd_proto::TraceChunk {
            priority: self.priority,
            origin: self.origin.clone(),
            spans: self.spans.clone(),
            tags: self.tags.clone(),
            dropped_trace: self.dropped,
        }
    }
}

struct AgentFields {
    host_name: String,
    env: String,
    agent_version: String,
    target_tps: f64,
    error_tps: f64,
    rare_sampler_enabled: bool,
    tags: BTreeMap<String, String>,
}

#[derive(Clone, Eq, PartialEq)]
struct AgentKey {
    host_name: String,
    env: String,
    agent_version: String,
    target_tps_bits: u64,
    error_tps_bits: u64,
    rare_sampler_enabled: bool,
    tags: BTreeMap<String, String>,
}

#[derive(Clone, Eq, PartialEq)]
struct TracerKey {
    host: Option<String>,
    environment: Option<String>,
    attributes: [Option<AttrValue>; TRACER_ATTRIBUTE_KEYS.len()],
    tags: BTreeMap<String, String>,
}

#[derive(Clone, Eq, PartialEq)]
struct ChunkKey {
    priority: i32,
    origin: Option<String>,
    dropped: bool,
    tags: BTreeMap<String, String>,
    trace_id: TraceId,
}

fn agent_parts(
    event: &TraceEvent,
    reporter: &mut impl TraceConversionReporter,
) -> (AgentKey, AgentFields) {
    let fields = match event.datadog().agent() {
        Some(agent) => AgentFields {
            host_name: agent.host_name.clone(),
            env: agent.env.clone(),
            agent_version: agent.agent_version.clone(),
            target_tps: agent.target_tps,
            error_tps: agent.error_tps,
            rare_sampler_enabled: agent.rare_sampler_enabled,
            tags: normalize_tags(&agent.tags, reporter),
        },
        None => AgentFields {
            host_name: String::new(),
            env: String::new(),
            agent_version: String::new(),
            target_tps: 0.0,
            error_tps: 0.0,
            rare_sampler_enabled: false,
            tags: BTreeMap::new(),
        },
    };
    let key = AgentKey {
        host_name: fields.host_name.clone(),
        env: fields.env.clone(),
        agent_version: fields.agent_version.clone(),
        target_tps_bits: fields.target_tps.to_bits(),
        error_tps_bits: fields.error_tps.to_bits(),
        rare_sampler_enabled: fields.rare_sampler_enabled,
        tags: fields.tags.clone(),
    };
    (key, fields)
}

fn tracer_key(event: &TraceEvent, reporter: &mut impl TraceConversionReporter) -> TracerKey {
    let resource = event.resource();
    TracerKey {
        host: resource.host.clone(),
        environment: resource.environment.clone(),
        attributes: TRACER_ATTRIBUTE_KEYS.map(|key| resource.attributes.get(key).cloned()),
        tags: normalize_tags(&event.datadog().tracer.tags, reporter),
    }
}

fn chunk_key(event: &TraceEvent, reporter: &mut impl TraceConversionReporter) -> ChunkKey {
    let chunk = event.datadog().chunk();
    ChunkKey {
        priority: chunk
            .and_then(|chunk| chunk.priority)
            .unwrap_or(SamplingPriority::AutoKeep)
            .to_i32(),
        origin: chunk.and_then(|chunk| chunk.origin.clone()),
        dropped: chunk.is_some_and(|chunk| chunk.dropped),
        tags: chunk
            .map(|chunk| normalize_tags(&chunk.tags, reporter))
            .unwrap_or_default(),
        trace_id: event.trace_id(),
    }
}

fn project_attr(
    attributes: &Attributes,
    key: &str,
    reporter: &mut impl TraceConversionReporter,
) -> String {
    match attributes.get(key) {
        None => String::new(),
        Some(AttrValue::Null) => {
            reporter.report(TraceConversionIssue::MalformedAttribute);
            String::new()
        }
        Some(AttrValue::String(value)) => value.clone(),
        Some(other) => dd_value_to_string(other),
    }
}

fn normalize_tags(
    attributes: &Attributes,
    reporter: &mut impl TraceConversionReporter,
) -> BTreeMap<String, String> {
    let mut tags = BTreeMap::new();
    for (key, value) in attributes.iter() {
        if matches!(value, AttrValue::Null) {
            reporter.report(TraceConversionIssue::MalformedAttribute);
            continue;
        }
        tags.insert(key.as_ref().to_owned(), dd_value_to_string(value));
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::group_typed_traces;
    use crate::{
        event::typed_trace::{
            AttrValue, DatadogAgentEnvelope, DatadogChunkContext, SamplingPriority,
        },
        sinks::datadog::traces::typed::testutil::{Issues, event, span},
    };

    fn group(
        events: Vec<crate::event::typed_trace::TraceEvent>,
    ) -> (Vec<super::AgentGroup>, Issues) {
        let mut issues = Issues::default();
        let groups = group_typed_traces(events, &mut issues);
        (groups, issues)
    }

    fn span_ids(group: &super::ChunkGroup) -> Vec<u64> {
        group.spans.iter().map(|span| span.span_id).collect()
    }

    #[test]
    fn null_tracer_attribute_is_reported() {
        let mut traced = event(1, Some("a"), vec![span(1, "a")]);
        traced
            .resource_mut()
            .attributes
            .insert("container.id", AttrValue::Null);
        let (groups, issues) = group(vec![traced]);
        assert_eq!(groups[0].tracers[0].container_id, "");
        assert_eq!(issues.malformed_attributes, 1);
        assert_eq!(issues.unexpected.len(), 0);
    }

    #[test]
    fn service_partitions_recoalesce_in_first_seen_order() {
        let (groups, issues) = group(vec![
            event(1, Some("a"), vec![span(1, "a1"), span(2, "a2")]),
            event(1, Some("b"), vec![span(3, "b1")]),
        ]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].tracers.len(), 1);
        assert_eq!(groups[0].tracers[0].chunks.len(), 1);
        let chunk = &groups[0].tracers[0].chunks[0];
        assert_eq!(span_ids(chunk), vec![1, 2, 3]);
        assert_eq!(chunk.event_indexes, vec![0, 1]);
        assert_eq!(chunk.priority, 1);
        assert_eq!(issues.unexpected.len(), 0);
        assert_eq!(issues.empty_events, 0);
    }

    #[test]
    fn repeated_service_starts_a_new_chunk() {
        let (groups, _) = group(vec![
            event(1, Some("a"), vec![span(1, "a1")]),
            event(1, Some("a"), vec![span(2, "a2")]),
            event(1, Some("b"), vec![span(3, "b1")]),
        ]);
        let chunks = &groups[0].tracers[0].chunks;
        assert_eq!(chunks.len(), 2);
        assert_eq!(span_ids(&chunks[0]), vec![1, 3]);
        assert_eq!(span_ids(&chunks[1]), vec![2]);
    }

    #[test]
    fn distinct_trace_ids_stay_in_distinct_chunks() {
        let (groups, _) = group(vec![
            event(1, Some("a"), vec![span(1, "a")]),
            event(2, Some("a"), vec![span(2, "a")]),
            event(1, Some("b"), vec![span(3, "b")]),
        ]);
        let chunks = &groups[0].tracers[0].chunks;
        assert_eq!(span_ids(&chunks[0]), vec![1, 3]);
        assert_eq!(chunks[0].trace_id.get(), 1);
        assert_eq!(span_ids(&chunks[1]), vec![2]);
        assert_eq!(chunks[1].trace_id.get(), 2);
    }

    #[test]
    fn absent_priority_is_auto_keep_and_explicit_reject_stays_apart() {
        let mut rejected = event(1, Some("b"), vec![span(2, "b")]);
        rejected.datadog_mut().set_chunk(Some(DatadogChunkContext {
            priority: Some(SamplingPriority::AutoReject),
            ..DatadogChunkContext::default()
        }));
        let mut explicit_keep = event(1, Some("c"), vec![span(3, "c")]);
        explicit_keep
            .datadog_mut()
            .set_chunk(Some(DatadogChunkContext {
                priority: Some(SamplingPriority::AutoKeep),
                ..DatadogChunkContext::default()
            }));
        let (groups, _) = group(vec![
            event(1, Some("a"), vec![span(1, "a")]),
            rejected,
            explicit_keep,
        ]);
        let chunks = &groups[0].tracers[0].chunks;
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].priority, 1);
        assert_eq!(span_ids(&chunks[0]), vec![1, 3]);
        assert_eq!(chunks[1].priority, 0);
        assert_eq!(span_ids(&chunks[1]), vec![2]);
    }

    #[test]
    fn default_chunk_shares_the_absent_priority_group() {
        let mut default_chunk = event(1, Some("b"), vec![span(2, "b")]);
        default_chunk
            .datadog_mut()
            .set_chunk(Some(DatadogChunkContext::default()));
        let (groups, _) = group(vec![event(1, Some("a"), vec![span(1, "a")]), default_chunk]);
        let chunks = &groups[0].tracers[0].chunks;
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].priority, 1);
        assert_eq!(span_ids(&chunks[0]), vec![1, 2]);
    }

    #[test]
    fn empty_events_emit_their_own_empty_chunks() {
        let (groups, issues) = group(vec![
            event(1, Some("a"), vec![span(1, "a")]),
            event(9, Some("a"), vec![]),
            event(8, Some("a"), vec![]),
        ]);
        let chunks = &groups[0].tracers[0].chunks;
        assert_eq!(chunks.len(), 3);
        assert_eq!(span_ids(&chunks[0]), vec![1]);
        assert_eq!(chunks[1].spans, []);
        assert_eq!(chunks[2].spans, []);
        assert_eq!(chunks[1].trace_id.get(), 9);
        assert_eq!(issues.empty_events, 2);
        let payload = groups[0].encode();
        assert_eq!(payload.tracer_payloads[0].chunks[1].spans, []);
        assert_eq!(payload.idx_tracer_payloads, []);
    }

    #[test]
    fn envelope_nan_bits_group_and_signed_zero_does_not() {
        let nan = f64::from_bits(0x7ff8_0000_0000_0001);
        let mut left = event(1, Some("a"), vec![span(1, "a")]);
        let mut right = event(1, Some("b"), vec![span(2, "b")]);
        left.datadog_mut().set_agent(Some(DatadogAgentEnvelope {
            target_tps: nan,
            ..DatadogAgentEnvelope::default()
        }));
        right.datadog_mut().set_agent(Some(DatadogAgentEnvelope {
            target_tps: nan,
            ..DatadogAgentEnvelope::default()
        }));
        let (groups, _) = group(vec![left, right]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].target_tps.to_bits(), nan.to_bits());

        let mut positive = event(1, Some("a"), vec![span(1, "a")]);
        let mut negative = event(1, Some("b"), vec![span(2, "b")]);
        positive.datadog_mut().set_agent(Some(DatadogAgentEnvelope {
            target_tps: 0.0,
            ..DatadogAgentEnvelope::default()
        }));
        negative.datadog_mut().set_agent(Some(DatadogAgentEnvelope {
            target_tps: -0.0,
            ..DatadogAgentEnvelope::default()
        }));
        let (groups, _) = group(vec![positive, negative]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].target_tps.to_bits(), 0.0f64.to_bits());
        assert_eq!(groups[1].target_tps.to_bits(), (-0.0f64).to_bits());
    }

    #[test]
    fn absent_agent_uses_proto_defaults_and_matches_an_explicit_default() {
        let mut explicit = event(1, Some("b"), vec![span(2, "b")]);
        explicit
            .datadog_mut()
            .set_agent(Some(DatadogAgentEnvelope::default()));
        let (groups, _) = group(vec![event(1, Some("a"), vec![span(1, "a")]), explicit]);
        assert_eq!(groups.len(), 1);
        let payload = groups[0].encode();
        assert_eq!(payload.host_name, "");
        assert_eq!(payload.env, "");
        assert_eq!(payload.tags.len(), 0);
        assert!(!payload.rare_sampler_enabled);
    }

    #[test]
    fn authoritative_agent_and_tracer_fields_split_groups() {
        let mut other_host = event(1, Some("b"), vec![span(2, "b")]);
        other_host
            .datadog_mut()
            .set_agent(Some(DatadogAgentEnvelope {
                host_name: "agent-b".into(),
                ..DatadogAgentEnvelope::default()
            }));
        let mut first = event(1, Some("a"), vec![span(1, "a")]);
        first.datadog_mut().set_agent(Some(DatadogAgentEnvelope {
            host_name: "agent-a".into(),
            ..DatadogAgentEnvelope::default()
        }));
        first.resource_mut().host = Some("tracer-host".into());
        first
            .resource_mut()
            .attributes
            .insert("container.id", AttrValue::String("cid".into()));
        first.resource_mut().schema_url = Some("https://schema".into());
        let mut same_tracer = event(1, Some("c"), vec![span(3, "c")]);
        same_tracer
            .datadog_mut()
            .set_agent(Some(DatadogAgentEnvelope {
                host_name: "agent-a".into(),
                ..DatadogAgentEnvelope::default()
            }));
        same_tracer.resource_mut().host = Some("tracer-host".into());
        same_tracer
            .resource_mut()
            .attributes
            .insert("container.id", AttrValue::String("cid".into()));
        same_tracer.resource_mut().schema_url = Some("https://other".into());
        same_tracer
            .resource_mut()
            .attributes
            .insert("region", AttrValue::String("eu".into()));
        first
            .resource_mut()
            .attributes
            .insert("region", AttrValue::String("us".into()));

        let (groups, _) = group(vec![first, other_host, same_tracer]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].host_name, "agent-a");
        assert_eq!(groups[1].host_name, "agent-b");
        assert_eq!(groups[0].tracers.len(), 1);
        assert_eq!(groups[0].tracers[0].hostname, "tracer-host");
        assert_eq!(groups[0].tracers[0].container_id, "cid");
        let payload = groups[0].encode();
        let spans = &payload.tracer_payloads[0].chunks[0].spans;
        assert_eq!(spans[0].meta.get("region").map(String::as_str), Some("us"));
        assert_eq!(spans[1].meta.get("region").map(String::as_str), Some("eu"));
        assert!(!spans[0].meta.contains_key("container.id"));
    }

    #[test]
    fn none_host_does_not_share_an_empty_host() {
        let mut empty = event(1, Some("b"), vec![span(2, "b")]);
        empty.resource_mut().host = Some(String::new());
        let (groups, _) = group(vec![event(1, Some("a"), vec![span(1, "a")]), empty]);
        assert_eq!(groups[0].tracers.len(), 2);
        assert_eq!(groups[0].tracers[0].hostname, "");
        assert_eq!(groups[0].tracers[1].hostname, "");
    }

    #[test]
    fn dropped_origin_and_chunk_tags_participate_in_the_key() {
        let mut dropped = event(1, Some("b"), vec![span(2, "b")]);
        dropped.datadog_mut().set_chunk(Some(DatadogChunkContext {
            dropped: true,
            origin: Some("rum".into()),
            tags: {
                let mut tags = crate::event::typed_trace::Attributes::new();
                tags.insert("x", AttrValue::Null);
                tags.insert("n", AttrValue::Float(f64::NAN));
                tags
            },
            ..DatadogChunkContext::default()
        }));
        let (groups, issues) = group(vec![event(1, Some("a"), vec![span(1, "a")]), dropped]);
        assert_eq!(groups[0].tracers[0].chunks.len(), 2);
        let chunk = &groups[0].tracers[0].chunks[1];
        assert!(chunk.dropped);
        assert_eq!(chunk.origin, "rum");
        assert_eq!(chunk.tags.get("n").map(String::as_str), Some("NaN"));
        assert!(!chunk.tags.contains_key("x"));
        assert_eq!(issues.malformed_attributes, 1);
    }

    #[test]
    fn agent_order_follows_first_seen_input() {
        let mut second = event(1, Some("a"), vec![span(2, "a")]);
        second.datadog_mut().set_agent(Some(DatadogAgentEnvelope {
            host_name: "b".into(),
            ..DatadogAgentEnvelope::default()
        }));
        let mut first = event(1, Some("a"), vec![span(1, "a")]);
        first.datadog_mut().set_agent(Some(DatadogAgentEnvelope {
            host_name: "a".into(),
            ..DatadogAgentEnvelope::default()
        }));
        let (groups, _) = group(vec![second, first]);
        assert_eq!(groups[0].host_name, "b");
        assert_eq!(groups[1].host_name, "a");
    }
}
