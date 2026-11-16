//! Wire-level oracle for RFC 25329's Datadog round trip, independent of typed codecs.
//!
//! References below are sections of `rfcs/2026-04-29-25329-trace-data-model/datadog-mapping.md`.
//! Only expected input is normalized. Actual output must already satisfy the egress contract.

use std::{collections::BTreeMap, fmt::Debug};

use datadog_proto::trace::{
    AgentPayload, AttributeAnyValue, AttributeArray, AttributeArrayValue, Span, SpanEvent,
    SpanLink, TraceChunk, TracerPayload,
};

type Fields = BTreeMap<String, String>;
/// Chunk identity ignores cross-service interleaving, but not span multiplicity or chunk boundaries.
type Identity = (u128, Vec<u64>);
type Records = BTreeMap<Identity, Vec<Fields>>;

fn put(fields: &mut Fields, path: &str, value: impl Debug) {
    fields.insert(path.into(), format!("{value:?}"));
}

macro_rules! scalars {
    ($fields:expr, $prefix:expr, $($name:ident),+ $(,)?) => {
        $(put($fields, &format!("{}.{}", $prefix, stringify!($name)), $name);)+
    };
}

fn map<T: Debug>(fields: &mut Fields, path: &str, values: &BTreeMap<String, T>) {
    for (key, value) in values {
        put(fields, &format!("{path}[{key:?}]"), value);
    }
}

fn double(fields: &mut Fields, path: &str, value: f64) {
    // "Envelope reconstruction and chunk re-coalescence": NaN payloads and signed zero matter.
    fields.insert(
        path.into(),
        format!("{value:?} (bits={:#018x})", value.to_bits()),
    );
}

fn attribute(fields: &mut Fields, path: &str, value: &AttributeAnyValue) {
    let AttributeAnyValue {
        r#type,
        string_value,
        bool_value,
        int_value,
        double_value,
        array_value,
    } = value;
    put(fields, &format!("{path}.type"), r#type);
    // The proto is a manually tagged union. Inactive slots do not describe a value.
    match r#type {
        0 => put(fields, path, string_value),
        1 => put(fields, path, bool_value),
        2 => put(fields, path, int_value),
        3 => double(fields, path, *double_value),
        4 => {
            let values = array_value
                .as_ref()
                .map(|AttributeArray { values }| values.as_slice())
                .unwrap_or_default();
            put(fields, &format!("{path}.length"), values.len());
            for (index, value) in values.iter().enumerate() {
                let path = format!("{path}[{index}]");
                let AttributeArrayValue {
                    r#type,
                    string_value,
                    bool_value,
                    int_value,
                    double_value,
                } = value;
                put(fields, &format!("{path}.type"), r#type);
                match r#type {
                    0 => put(fields, &path, string_value),
                    1 => put(fields, &path, bool_value),
                    2 => put(fields, &path, int_value),
                    3 => double(fields, &path, *double_value),
                    _ => put(fields, &path, value),
                }
            }
        }
        _ => put(fields, path, value),
    }
}

fn span_fields(fields: &mut Fields, path: &str, span: &Span) {
    // Exhaustive destructuring makes additions to the wire schema require an oracle decision.
    let Span {
        service,
        name,
        resource,
        trace_id,
        span_id,
        parent_id,
        start,
        duration,
        error,
        meta,
        metrics,
        r#type,
        meta_struct,
        span_links,
        span_events,
    } = span;
    scalars!(
        fields, path, service, name, resource, trace_id, span_id, parent_id, start, duration,
        error, r#type
    );
    map(fields, &format!("{path}.meta"), meta);
    map(fields, &format!("{path}.meta_struct"), meta_struct);
    for (key, value) in metrics {
        double(fields, &format!("{path}.metrics[{key:?}]"), *value);
    }
    put(fields, &format!("{path}.links.length"), span_links.len());
    for (index, link) in span_links.iter().enumerate() {
        let SpanLink {
            trace_id,
            trace_id_high,
            span_id,
            attributes,
            tracestate,
            flags,
        } = link;
        let path = format!("{path}.links[{index}]");
        scalars!(
            fields,
            path,
            trace_id,
            trace_id_high,
            span_id,
            tracestate,
            flags
        );
        map(fields, &format!("{path}.attributes"), attributes);
    }
    put(fields, &format!("{path}.events.length"), span_events.len());
    for (index, event) in span_events.iter().enumerate() {
        let SpanEvent {
            time_unix_nano,
            name,
            attributes,
        } = event;
        let path = format!("{path}.events[{index}]");
        scalars!(fields, path, time_unix_nano, name);
        for (key, value) in attributes {
            attribute(fields, &format!("{path}.attributes[{key:?}]"), value);
        }
    }
}

fn envelope(agent: &AgentPayload, tracer: &TracerPayload, chunk: &TraceChunk) -> Fields {
    let mut fields = Fields::new();
    let AgentPayload {
        host_name,
        env,
        agent_version,
        target_tps,
        error_tps,
        rare_sampler_enabled,
        tags,
        tracer_payloads: _,
        idx_tracer_payloads: _,
    } = agent;
    scalars!(
        &mut fields,
        "agent",
        host_name,
        env,
        agent_version,
        rare_sampler_enabled
    );
    double(&mut fields, "agent.target_tps", *target_tps);
    double(&mut fields, "agent.error_tps", *error_tps);
    map(&mut fields, "agent.tags", tags);
    let TracerPayload {
        container_id,
        language_name,
        language_version,
        tracer_version,
        runtime_id,
        env,
        hostname,
        app_version,
        tags,
        chunks: _,
        container_debug: _,
    } = tracer;
    scalars!(
        &mut fields,
        "tracer",
        container_id,
        language_name,
        language_version,
        tracer_version,
        runtime_id,
        env,
        hostname,
        app_version
    );
    map(&mut fields, "tracer.tags", tags);
    // "Out of scope": containerDebug is a diagnostic, not part of the relay guarantee.
    let TraceChunk {
        priority,
        origin,
        dropped_trace,
        tags,
        spans: _,
    } = chunk;
    scalars!(&mut fields, "chunk", priority, origin, dropped_trace);
    map(&mut fields, "chunk.tags", tags);
    fields
}

fn high(span: &Span) -> Result<Option<u64>, String> {
    match span.meta.get("_dd.p.tid").filter(|value| !value.is_empty()) {
        None => Ok(None),
        Some(value) if value.len() <= 16 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) => {
            u64::from_str_radix(value, 16)
                .map(Some)
                .map_err(|error| error.to_string())
        }
        Some(value) => Err(format!("malformed _dd.p.tid: {value:?}")),
    }
}

fn mechanism(value: &str) -> Option<u32> {
    let digits = value.strip_prefix('-').unwrap_or(value);
    (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| digits.parse().ok())
        .flatten()
}

fn expected_chunk(chunk: &TraceChunk) -> TraceChunk {
    let mut expected = chunk.clone();
    // "Sampling mechanism": chunk tag wins, else the first span carrying the key.
    let sampling = chunk
        .tags
        .get("_dd.p.dm")
        .or_else(|| {
            chunk
                .spans
                .iter()
                .find_map(|span| span.meta.get("_dd.p.dm"))
        })
        .and_then(|value| mechanism(value));
    expected.tags.remove("_dd.p.dm");
    if let Some(value) = sampling {
        expected.tags.insert("_dd.p.dm".into(), format!("-{value}"));
    }
    let mut tagged = BTreeMap::<u64, Vec<u64>>::new();
    for span in &chunk.spans {
        if let Ok(Some(high)) = high(span) {
            let highs = tagged.entry(span.trace_id).or_default();
            if !highs.contains(&high) {
                highs.push(high);
            }
        }
    }
    expected.spans.retain_mut(|span| {
        // "Ingress and egress mapping" and parent RFC "Identifiers": malformed/zero IDs drop spans.
        let Ok(high) = high(span) else { return false };
        let high = match (high, tagged.get(&span.trace_id).map(Vec::as_slice)) {
            (Some(high), _) => high,
            (None, Some([high])) => *high,
            (None, Some([_, _, ..])) => return false,
            (None, _) => 0,
        };
        if (high == 0 && span.trace_id == 0) || span.span_id == 0 {
            return false;
        }
        let message = span.meta.get("error.message").cloned();
        let kind = span
            .meta
            .get("span.kind")
            .filter(|kind| {
                matches!(
                    kind.as_str(),
                    "internal" | "server" | "client" | "producer" | "consumer"
                )
            })
            .cloned();
        // "Datadog attribute partitions": metrics wins scalar-map collisions, not meta_struct.
        span.meta.retain(|key, _| !span.metrics.contains_key(key));
        // "Signed timing fields on ingress" and "Span.error and Span.status".
        span.duration = span.duration.max(0);
        span.error = i32::from(span.error != 0);
        // "_dd.p.tid": sink-owned, canonical hex on every span with a non-zero high half.
        span.meta.remove("_dd.p.tid");
        span.metrics.remove("_dd.p.tid");
        if high != 0 {
            span.meta.insert("_dd.p.tid".into(), format!("{high:016x}"));
        }
        // "Sampling mechanism": distribution, spelling and unparseable values are declared exclusions.
        span.meta.remove("_dd.p.dm");
        span.metrics.remove("_dd.p.dm");
        if let Some(value) = sampling {
            span.meta.insert("_dd.p.dm".into(), format!("-{value}"));
        }
        // "Span.kind": a lifted known kind owns the key on egress, including its partition.
        if let Some(kind) = kind {
            span.metrics.remove("span.kind");
            span.meta.insert("span.kind".into(), kind);
        }
        // "Datadog egress derivation rules": non-empty error status owns error.message.
        if span.error != 0
            && let Some(message) = message.filter(|message| !message.is_empty())
        {
            span.metrics.remove("error.message");
            span.meta.insert("error.message".into(), message);
        }
        // Parent RFC "Identifiers": links use the combined 128-bit ID; parentID=0 is valid.
        span.span_links
            .retain(|link| (link.trace_id != 0 || link.trace_id_high != 0) && link.span_id != 0);
        // "SpanEvent.attributes typed value mapping": drop only an unknown-tagged attribute.
        for event in &mut span.span_events {
            event.attributes.retain(|_, value| {
                (0..=4).contains(&value.r#type)
                    && (value.r#type != 4
                        || value.array_value.as_ref().is_none_or(|array| {
                            array
                                .values
                                .iter()
                                .all(|value| (0..=3).contains(&value.r#type))
                        }))
            });
        }
        true
    });
    expected
}

fn records(payloads: &[AgentPayload], expected: bool) -> Result<Records, String> {
    let mut records = Records::new();
    for agent in payloads {
        if !agent.idx_tracer_payloads.is_empty() {
            return Err("The standard-payload contract does not accept idxTracerPayloads".into());
        }
        for tracer in &agent.tracer_payloads {
            for original in &tracer.chunks {
                let normalized;
                let chunk = if expected {
                    normalized = expected_chunk(original);
                    &normalized
                } else {
                    original
                };
                let mut traces = BTreeMap::<u128, BTreeMap<&str, Vec<&Span>>>::new();
                for span in &chunk.spans {
                    let high = high(span).map_err(|error| {
                        format!(
                            "trace={:016x} span={:016x}: {error}",
                            span.trace_id, span.span_id
                        )
                    })?;
                    let id = (u128::from(high.unwrap_or(0)) << 64) | u128::from(span.trace_id);
                    traces
                        .entry(id)
                        .or_default()
                        .entry(&span.service)
                        .or_default()
                        .push(span);
                }
                // "Ingress and egress mapping": empty payloads, tracers and chunks have no events.
                for (trace_id, services) in traces {
                    let mut fields = envelope(agent, tracer, chunk);
                    let mut identities = Vec::new();
                    // "Scope": ignore cross-service interleaving, preserve order within each partition.
                    for (service, spans) in services {
                        for (index, span) in spans.iter().enumerate() {
                            identities.push(span.span_id);
                            span_fields(&mut fields, &format!("spans[{service:?}][{index}]"), span);
                        }
                    }
                    identities.sort_unstable();
                    records
                        .entry((trace_id, identities))
                        .or_default()
                        .push(fields);
                }
            }
        }
    }
    for occurrences in records.values_mut() {
        occurrences.sort();
    }
    Ok(records)
}

/// Compares input-derived RFC expectations with unnormalized output, including duplicate chunks.
pub(super) fn compare(
    input: &[AgentPayload],
    output: &[AgentPayload],
) -> Result<Vec<String>, String> {
    let expected = records(input, true)?;
    let actual = records(output, false)?;
    let mut differences = Vec::new();
    for identity in expected
        .keys()
        .chain(actual.keys())
        .collect::<std::collections::BTreeSet<_>>()
    {
        let left = expected
            .get(identity)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let right = actual.get(identity).map(Vec::as_slice).unwrap_or_default();
        let context = format!("trace={:032x} chunk_spans={:?}", identity.0, identity.1);
        if left.len() != right.len() {
            differences.push(format!(
                "{context}: chunk occurrences: expected {}, actual {}",
                left.len(),
                right.len()
            ));
        }
        for (index, (left, right)) in left.iter().zip(right).enumerate() {
            for path in left
                .keys()
                .chain(right.keys())
                .collect::<std::collections::BTreeSet<_>>()
            {
                if left.get(path) != right.get(path) {
                    differences.push(format!(
                        "{context} occurrence={index} {path}: expected {}, actual {}",
                        left.get(path).map_or("<absent>", String::as_str),
                        right.get(path).map_or("<absent>", String::as_str)
                    ));
                }
            }
        }
    }
    Ok(differences)
}

#[cfg(test)]
mod tests;
