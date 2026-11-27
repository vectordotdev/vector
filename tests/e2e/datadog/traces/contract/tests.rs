use datadog_proto::trace::{AttributeArray, AttributeArrayValue, ContainerDebug};

use super::*;

fn span(id: u64, service: &str) -> Span {
    Span {
        trace_id: u64::MAX,
        span_id: id,
        service: service.into(),
        name: "request".into(),
        resource: String::new(),
        r#type: String::new(),
        start: -42,
        duration: 123,
        meta: BTreeMap::from([("_dd.p.tid".into(), "abcdef0123456789".into())]),
        ..Default::default()
    }
}

fn payload() -> Vec<AgentPayload> {
    vec![AgentPayload {
        host_name: "agent-host".into(),
        env: "agent-env".into(),
        agent_version: "agent-version".into(),
        target_tps: 12.5,
        error_tps: 3.5,
        rare_sampler_enabled: true,
        tags: BTreeMap::from([("same-key".into(), "agent".into())]),
        tracer_payloads: vec![TracerPayload {
            hostname: "tracer-host".into(),
            env: "tracer-env".into(),
            container_id: "container".into(),
            language_name: "language".into(),
            language_version: "language-version".into(),
            tracer_version: "tracer-version".into(),
            runtime_id: "runtime".into(),
            app_version: "app-version".into(),
            tags: BTreeMap::from([("same-key".into(), "tracer".into())]),
            chunks: vec![TraceChunk {
                priority: 0,
                origin: "origin".into(),
                dropped_trace: true,
                tags: BTreeMap::from([("same-key".into(), "chunk".into())]),
                spans: vec![span(1, "web"), span(2, "db"), span(3, "web")],
            }],
            ..Default::default()
        }],
        ..Default::default()
    }]
}

fn chunk(payloads: &mut [AgentPayload]) -> &mut TraceChunk {
    &mut payloads[0].tracer_payloads[0].chunks[0]
}

fn unchanged(input: &[AgentPayload], output: &[AgentPayload]) {
    assert_eq!(compare(input, output).unwrap(), Vec::<String>::new());
}

fn changed(input: &[AgentPayload], output: &[AgentPayload], path: &str) {
    let differences = compare(input, output).unwrap();
    assert!(
        differences
            .iter()
            .any(|difference| difference.contains(path)),
        "{differences:#?}"
    );
}

#[test]
fn identity_and_cross_service_reordering() {
    let input = payload();
    unchanged(&input, &input);
    let mut output = input.clone();
    chunk(&mut output).spans.swap(1, 2);
    unchanged(&input, &output);
    chunk(&mut output).spans.swap(0, 1);
    changed(&input, &output, "span_id");
}

#[test]
fn agent_and_tracer_regrouping_and_multi_trace_chunk_split() {
    let mut input = payload();
    let mut second = input[0].tracer_payloads[0].clone();
    second.chunks[0].spans[0].trace_id = 15;
    second.chunks[0].spans[1].trace_id = 15;
    input[0].tracer_payloads.push(second);
    let mut output = input.clone();
    let tracer = output[0].tracer_payloads.remove(1);
    let mut agent = output[0].clone();
    agent.tracer_payloads = vec![tracer];
    output.push(agent);
    let original = output[1].tracer_payloads[0].chunks.remove(0);
    for trace in [15, u64::MAX] {
        let mut split = original.clone();
        split.spans.retain(|span| span.trace_id == trace);
        output[1].tracer_payloads[0].chunks.push(split);
    }
    output.reverse();
    unchanged(&input, &output);
}

#[test]
fn distinct_chunks_and_duplicate_occurrences_are_not_discarded() {
    let input = payload();
    let mut output = input.clone();
    let extra = chunk(&mut output).clone();
    output[0].tracer_payloads[0].chunks.push(extra);
    changed(&input, &output, "chunk occurrences");
    unchanged(&output, &output);
    let mut merged = output.clone();
    let extra = merged[0].tracer_payloads[0].chunks.remove(1);
    chunk(&mut merged).spans.extend(extra.spans);
    changed(&output, &merged, "chunk occurrences");
    let mut missing = input.clone();
    chunk(&mut missing).spans.remove(0);
    changed(&input, &missing, "chunk occurrences");
}

#[test]
fn every_envelope_scalar_and_tag_scope_is_compared() {
    let input = payload();
    let mutations: &[fn(&mut AgentPayload)] = &[
        |p| p.host_name.clear(),
        |p| p.env.clear(),
        |p| p.agent_version.clear(),
        |p| p.target_tps = 0.0,
        |p| p.error_tps = 0.0,
        |p| p.rare_sampler_enabled = false,
        |p| p.tags.clear(),
        |p| p.tracer_payloads[0].hostname.clear(),
        |p| p.tracer_payloads[0].env.clear(),
        |p| p.tracer_payloads[0].container_id.clear(),
        |p| p.tracer_payloads[0].language_name.clear(),
        |p| p.tracer_payloads[0].language_version.clear(),
        |p| p.tracer_payloads[0].tracer_version.clear(),
        |p| p.tracer_payloads[0].runtime_id.clear(),
        |p| p.tracer_payloads[0].app_version.clear(),
        |p| p.tracer_payloads[0].tags.clear(),
        |p| p.tracer_payloads[0].chunks[0].priority = 1,
        |p| p.tracer_payloads[0].chunks[0].origin.clear(),
        |p| p.tracer_payloads[0].chunks[0].dropped_trace = false,
        |p| p.tracer_payloads[0].chunks[0].tags.clear(),
    ];
    for (index, mutate) in mutations.iter().enumerate() {
        let mut output = input.clone();
        mutate(&mut output[0]);
        assert!(
            !compare(&input, &output).unwrap().is_empty(),
            "mutation {index}"
        );
    }
    let mut output = input.clone();
    output[0].tracer_payloads[0].env = input[0].env.clone();
    changed(&input, &output, "tracer.env");
    let mut output = input.clone();
    output[0].tracer_payloads[0]
        .tags
        .extend(input[0].tags.clone());
    changed(&input, &output, "tracer.tags[\"same-key\"]");
}

#[test]
fn nested_fields_empty_strings_and_numeric_bits_survive() {
    let mut input = payload();
    let first = &mut chunk(&mut input).spans[0];
    first
        .metrics
        .insert("nan".into(), f64::from_bits(0x7ff8_0000_0000_0042));
    first.meta_struct.insert("opaque".into(), vec![0, 255, 128]);
    first.span_links.push(SpanLink {
        trace_id: 0,
        trace_id_high: 42,
        span_id: u64::MAX,
        flags: 0x8000_0001,
        tracestate: "arbitrary=verbatim".into(),
        ..Default::default()
    });
    first.span_events.push(SpanEvent {
        time_unix_nano: u64::MAX,
        name: "event".into(),
        attributes: BTreeMap::from([(
            "values".into(),
            AttributeAnyValue {
                r#type: 4,
                array_value: Some(AttributeArray {
                    values: vec![AttributeArrayValue {
                        r#type: 3,
                        double_value: -0.0,
                        ..Default::default()
                    }],
                }),
                ..Default::default()
            },
        )]),
    });
    input[0].target_tps = f64::from_bits(0x7ff8_0000_0000_0042);
    input[0].error_tps = -0.0;
    unchanged(&input, &input);
    let mut output = input.clone();
    output[0].target_tps = f64::from_bits(0x7ff8_0000_0000_0043);
    changed(&input, &output, "agent.target_tps");
    let mut output = input.clone();
    output[0].error_tps = 0.0;
    changed(&input, &output, "agent.error_tps");
    let mutations: &[fn(&mut Span)] = &[
        |s| s.name.clear(),
        |s| s.resource = "fallback-is-wrong".into(),
        |s| s.r#type = "fallback-is-wrong".into(),
        |s| s.parent_id = 42,
        |s| s.start = 0,
        |s| s.duration += 1,
        |s| s.error = 1,
        |s| s.meta_struct.clear(),
        |s| {
            s.metrics.insert("nan".into(), f64::NAN);
        },
        |s| s.span_links[0].trace_id_high = 43,
        |s| s.span_links[0].flags = 1,
        |s| s.span_links[0].tracestate.clear(),
        |s| s.span_events[0].time_unix_nano = 0,
        |s| {
            s.span_events[0]
                .attributes
                .get_mut("values")
                .unwrap()
                .array_value
                .as_mut()
                .unwrap()
                .values[0]
                .double_value = 0.0;
        },
    ];
    for (index, mutate) in mutations.iter().enumerate() {
        let mut output = input.clone();
        mutate(&mut chunk(&mut output).spans[0]);
        assert!(
            !compare(&input, &output).unwrap().is_empty(),
            "mutation {index}"
        );
    }
}

#[test]
fn declared_normalizations_only_apply_to_expected_input() {
    let mut input = payload();
    let source = chunk(&mut input);
    source.spans[0].error = 42;
    source.spans[0].duration = -1;
    source.spans[0]
        .meta
        .insert("collision".into(), "lost".into());
    source.spans[0].metrics.insert("collision".into(), 2.0);
    source.spans[0].meta.insert("_dd.p.dm".into(), "3".into());
    source.spans[1].meta.insert("_dd.p.dm".into(), "-9".into());
    source.spans[1].meta.remove("_dd.p.tid");
    source.spans[2]
        .meta
        .insert("_dd.p.tid".into(), "ABCDEF0123456789".into());
    let mut output = input.clone();
    let actual = chunk(&mut output);
    actual.tags.insert("_dd.p.dm".into(), "-3".into());
    for span in &mut actual.spans {
        span.meta.insert("_dd.p.dm".into(), "-3".into());
        span.meta
            .insert("_dd.p.tid".into(), "abcdef0123456789".into());
    }
    actual.spans[0].error = 1;
    actual.spans[0].duration = 0;
    actual.spans[0].meta.remove("collision");
    unchanged(&input, &output);
    // Normalizing actual would incorrectly allow the broken encoder to pass these.
    let mut broken = output.clone();
    chunk(&mut broken).spans[0].duration = -1;
    changed(&input, &broken, "duration");
    let mut broken = output.clone();
    chunk(&mut broken).spans[1].meta.remove("_dd.p.dm");
    changed(&input, &broken, "_dd.p.dm");
}

#[test]
fn chunk_sampling_tag_wins_and_invalid_sampling_is_removed() {
    for (tag, expected) in [("-7", Some("-7")), ("invalid", None)] {
        let mut input = payload();
        chunk(&mut input).tags.insert("_dd.p.dm".into(), tag.into());
        chunk(&mut input).spans[0]
            .meta
            .insert("_dd.p.dm".into(), "-3".into());
        let mut output = input.clone();
        let actual = chunk(&mut output);
        actual.tags.remove("_dd.p.dm");
        for span in &mut actual.spans {
            span.meta.remove("_dd.p.dm");
        }
        if let Some(expected) = expected {
            actual.tags.insert("_dd.p.dm".into(), expected.into());
            for span in &mut actual.spans {
                span.meta.insert("_dd.p.dm".into(), expected.into());
            }
        }
        unchanged(&input, &output);
    }
}

#[test]
fn malformed_identity_and_unknown_event_values_drop_only_the_declared_items() {
    let mut input = payload();
    let source = chunk(&mut input);
    source.spans[1].meta.insert("_dd.p.tid".into(), "1".into());
    source.spans[2].meta.remove("_dd.p.tid"); // Ambiguous untagged sibling.
    source.spans[0].span_links = vec![SpanLink {
        trace_id: 1,
        span_id: 0,
        ..Default::default()
    }];
    source.spans[0].span_events = vec![SpanEvent {
        attributes: BTreeMap::from([
            (
                "valid".into(),
                AttributeAnyValue {
                    string_value: "kept".into(),
                    ..Default::default()
                },
            ),
            (
                "unknown".into(),
                AttributeAnyValue {
                    r#type: 999,
                    ..Default::default()
                },
            ),
            (
                "unknown-array".into(),
                AttributeAnyValue {
                    r#type: 4,
                    array_value: Some(AttributeArray {
                        values: vec![AttributeArrayValue {
                            r#type: 999,
                            ..Default::default()
                        }],
                    }),
                    ..Default::default()
                },
            ),
        ]),
        ..Default::default()
    }];
    let mut output = input.clone();
    let actual = chunk(&mut output);
    actual.spans.pop();
    actual.spans[1]
        .meta
        .insert("_dd.p.tid".into(), "0000000000000001".into());
    actual.spans[0].span_links.clear();
    actual.spans[0].span_events[0]
        .attributes
        .retain(|key, _| key == "valid");
    unchanged(&input, &output);
    chunk(&mut output).spans[0].span_events[0]
        .attributes
        .clear();
    changed(&input, &output, "valid");
}

#[test]
fn standard_empty_shapes_and_container_diagnostic_are_not_span_data() {
    let mut input = payload();
    input[0].tracer_payloads[0].container_debug = Some(ContainerDebug::default());
    input.push(AgentPayload::default());
    input[0].tracer_payloads.push(TracerPayload::default());
    input[0].tracer_payloads[0]
        .chunks
        .push(TraceChunk::default());
    unchanged(&input, &payload());
    let mut indexed = payload();
    indexed[0]
        .idx_tracer_payloads
        .push(datadog_proto::trace::idx::TracerPayload::default());
    assert!(
        compare(&indexed, &payload())
            .unwrap_err()
            .contains("idxTracerPayloads")
    );
    assert!(compare(&payload(), &indexed).is_err());
}

#[test]
fn zero_and_malformed_ids_follow_the_smallest_item_rule() {
    let mut input = payload();
    let source = chunk(&mut input);
    source.spans[0].trace_id = 0; // A non-zero high half makes this a valid trace ID.
    source.spans[1].span_id = 0;
    source.spans[2]
        .meta
        .insert("_dd.p.tid".into(), "invalid".into());
    source.spans.push(Span {
        trace_id: 0,
        span_id: 4,
        meta: BTreeMap::from([("_dd.p.tid".into(), "0".into())]),
        ..Default::default()
    });
    let mut output = input.clone();
    chunk(&mut output).spans.truncate(1);
    unchanged(&input, &output);
    chunk(&mut output).spans[0].parent_id = 42;
    changed(&input, &output, "parent_id");
}

#[test]
fn sink_owned_keys_and_scalar_collisions_do_not_erase_structured_attributes() {
    let mut input = payload();
    let source = &mut chunk(&mut input).spans[0];
    source.error = 1;
    for (key, value) in [
        ("span.kind", "server"),
        ("error.message", "failure"),
        ("ordinary", "lost"),
    ] {
        source.meta.insert(key.into(), value.into());
        source.metrics.insert(key.into(), 1.0);
        source.meta_struct.insert(key.into(), vec![255]);
    }
    let mut output = input.clone();
    let actual = &mut chunk(&mut output).spans[0];
    actual.meta.remove("ordinary");
    actual.metrics.remove("span.kind");
    actual.metrics.remove("error.message");
    unchanged(&input, &output);
    chunk(&mut output).spans[0].meta_struct.clear();
    changed(&input, &output, "meta_struct");
}

#[test]
fn absent_error_message_and_unknown_span_kind_are_not_rewritten() {
    let mut input = payload();
    let source = &mut chunk(&mut input).spans[0];
    source.error = 1;
    source
        .meta
        .insert("span.kind".into(), "not-a-known-kind".into());
    unchanged(&input, &input);
    let mut output = input.clone();
    chunk(&mut output).spans[0]
        .meta
        .insert("error.message".into(), String::new());
    changed(&input, &output, "error.message");
}
