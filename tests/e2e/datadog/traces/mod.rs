//! Validates native tracer traffic relayed by the `datadog_agent` source and the
//! `datadog_traces` sink. See `tests/e2e/datadog-traces/README.md`.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use base64::{Engine, prelude::BASE64_STANDARD};
use datadog_proto::trace::{AgentPayload, Span};
use prost::Message;
use serde::Serialize;
use vector_common::decompression::CappedDecoder;

use super::*;

const OUTPUT: &str = "/output/datadog-traces";
const TRACES_ENDPOINT: &str = "/api/v0.2/traces";
/// Differences shown in the error, so CI logs are debuggable without the evidence volume.
const LOGGED_DIFFERENCES: usize = 3;
const LOGGED_DIFFERENCE_BYTES: usize = 4_000;
const REQUIRED_TRACERS: &[&str] = &["go", "nodejs", "python"];
/// Checkouts sent by `workload/python/client.py`.
const CHECKOUTS: usize = 6;
const CHECKOUT_RESOURCE: &str = "GET /checkout/<int:order>";
const REQUIRED_SERVICES: &[&str] = &[
    "frontend",
    "frontend-cache",
    "pricing",
    "inventory",
    "inventory-db",
];

fn decode_payloads(response: &FakeIntakeResponseRaw) -> Result<Vec<AgentPayload>, String> {
    response
        .payloads
        .iter()
        .map(|payload| {
            let body = BASE64_STANDARD
                .decode(&payload.data)
                .map_err(|error| format!("invalid base64 payload: {error}"))?;
            let body = match payload.encoding.as_str() {
                "" | "identity" => body,
                "gzip" => CappedDecoder::gzip(body.as_slice())
                    .decompress()
                    .map_err(|error| format!("invalid gzip payload: {error}"))?,
                "zstd" => CappedDecoder::zstd(body.as_slice())
                    .and_then(CappedDecoder::decompress)
                    .map_err(|error| format!("invalid zstd payload: {error}"))?,
                encoding => return Err(format!("unsupported payload encoding: {encoding}")),
            };
            AgentPayload::decode(body.as_slice())
                .map_err(|error| format!("invalid AgentPayload: {error}"))
        })
        .collect()
}

const TRACE_ID_HIGH_TAG: &str = "_dd.p.tid";

/// Full 128-bit trace ID and `Span.spanID`.
type SpanKey = (u128, u64);

#[derive(Clone, Copy, PartialEq, Eq)]
enum TidTag {
    Absent,
    Valid(u64),
    Invalid,
}

fn parse_tid_tag(span: &Span) -> TidTag {
    let Some(value) = span.meta.get(TRACE_ID_HIGH_TAG) else {
        return TidTag::Absent;
    };
    if value.is_empty() {
        return TidTag::Valid(0);
    }
    if value.len() > 16 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return TidTag::Invalid;
    }
    u64::from_str_radix(value, 16).map_or(TidTag::Invalid, TidTag::Valid)
}

fn combine_trace_id(high: u64, low: u64) -> u128 {
    (u128::from(high) << 64) | u128::from(low)
}

struct ExpectedOccurrence {
    normalized: Vec<u8>,
    /// Legacy pre-RFC sinks leave untagged child spans without `_dd.p.tid`.
    legacy_untagged: Option<(SpanKey, Vec<u8>)>,
}

/// Every expected span with its occurrences, resolving chunk-scoped 128-bit trace IDs and
/// canonicalizing `_dd.p.tid` per RFC 25329 before flattening.
fn input_spans(payloads: &[AgentPayload]) -> BTreeMap<SpanKey, Vec<ExpectedOccurrence>> {
    let mut spans: BTreeMap<SpanKey, Vec<ExpectedOccurrence>> = BTreeMap::new();
    for chunk in payloads
        .iter()
        .flat_map(|payload| &payload.tracer_payloads)
        .flat_map(|tracer| &tracer.chunks)
    {
        let mut tagged_by_low: BTreeMap<u64, BTreeSet<u128>> = BTreeMap::new();
        for span in &chunk.spans {
            if let TidTag::Valid(high) = parse_tid_tag(span) {
                tagged_by_low
                    .entry(span.trace_id)
                    .or_default()
                    .insert(combine_trace_id(high, span.trace_id));
            }
        }
        for span in &chunk.spans {
            let tag = parse_tid_tag(span);
            let combined = match tag {
                TidTag::Invalid => continue,
                TidTag::Valid(high) => combine_trace_id(high, span.trace_id),
                TidTag::Absent => match tagged_by_low.get(&span.trace_id) {
                    Some(ids) if ids.len() == 1 => *ids.iter().next().expect("len == 1"),
                    Some(_) => continue,
                    None => u128::from(span.trace_id),
                },
            };
            if combined == 0 {
                continue;
            }
            let high = (combined >> 64) as u64;
            let mut normalized = span.clone();
            if high != 0 {
                normalized
                    .meta
                    .insert(TRACE_ID_HIGH_TAG.into(), format!("{high:016x}"));
            } else {
                normalized.meta.remove(TRACE_ID_HIGH_TAG);
            }
            let legacy_untagged = (tag == TidTag::Absent && high != 0).then(|| {
                (
                    (u128::from(span.trace_id), span.span_id),
                    span.encode_to_vec(),
                )
            });
            spans
                .entry((combined, normalized.span_id))
                .or_default()
                .push(ExpectedOccurrence {
                    normalized: normalized.encode_to_vec(),
                    legacy_untagged,
                });
        }
    }
    for occurrences in spans.values_mut() {
        occurrences.sort_by(|left, right| left.normalized.cmp(&right.normalized));
    }
    spans
}

/// Every actual output span with its occurrences, keyed by its own 128-bit trace ID without
/// sibling inheritance or tag stripping.
fn output_spans(payloads: &[AgentPayload]) -> BTreeMap<SpanKey, Vec<Vec<u8>>> {
    let mut spans: BTreeMap<SpanKey, Vec<Vec<u8>>> = BTreeMap::new();
    for span in payloads
        .iter()
        .flat_map(|payload| &payload.tracer_payloads)
        .flat_map(|tracer| &tracer.chunks)
        .flat_map(|chunk| &chunk.spans)
    {
        let combined = match parse_tid_tag(span) {
            TidTag::Valid(high) => combine_trace_id(high, span.trace_id),
            TidTag::Absent | TidTag::Invalid => u128::from(span.trace_id),
        };
        spans
            .entry((combined, span.span_id))
            .or_default()
            .push(span.encode_to_vec());
    }
    for occurrences in spans.values_mut() {
        occurrences.sort();
    }
    spans
}

fn take_matching_occurrence(occurrences: &mut Vec<Vec<u8>>, target: &[u8]) -> bool {
    if let Some(index) = occurrences
        .iter()
        .position(|item| item.as_slice() == target)
    {
        occurrences.remove(index);
        true
    } else {
        false
    }
}

fn compare(input: &[AgentPayload], output: &[AgentPayload]) -> Vec<String> {
    let input = input_spans(input);
    let mut actual_by_key = output_spans(output);
    let mut unmatched_expected: BTreeMap<SpanKey, Vec<Vec<u8>>> = BTreeMap::new();

    for (key, expected_occurrences) in input {
        for occurrence in expected_occurrences {
            let matched_normalized = actual_by_key
                .get_mut(&key)
                .is_some_and(|actual| take_matching_occurrence(actual, &occurrence.normalized));
            if matched_normalized {
                continue;
            }
            let matched_legacy =
                occurrence
                    .legacy_untagged
                    .as_ref()
                    .is_some_and(|(legacy_key, legacy_bytes)| {
                        actual_by_key
                            .get_mut(legacy_key)
                            .is_some_and(|actual| take_matching_occurrence(actual, legacy_bytes))
                    });
            if !matched_legacy {
                unmatched_expected
                    .entry(key)
                    .or_default()
                    .push(occurrence.normalized);
            }
        }
    }
    actual_by_key.retain(|_, occurrences| !occurrences.is_empty());

    let keys: BTreeSet<_> = unmatched_expected
        .keys()
        .chain(actual_by_key.keys())
        .copied()
        .collect();
    keys.into_iter()
        .map(|key| {
            let expected = unmatched_expected.remove(&key).unwrap_or_default();
            let actual = actual_by_key.remove(&key).unwrap_or_default();
            let decode = |spans: Vec<Vec<u8>>| {
                spans
                    .iter()
                    .map(|span| Span::decode(span.as_slice()).unwrap())
                    .collect::<Vec<_>>()
            };
            format!(
                "trace={:032x} span={:016x}\nexpected: {:#?}\nactual: {:#?}",
                key.0,
                key.1,
                decode(expected),
                decode(actual),
            )
        })
        .collect()
}

#[derive(Default, Serialize)]
struct Coverage {
    tracers: BTreeMap<String, usize>,
    services: BTreeMap<String, usize>,
    priorities: BTreeMap<i32, usize>,
    spans: usize,
    traces: usize,
    /// Checkout traces, and those with spans from every required tracer.
    checkouts: usize,
    complete_checkouts: usize,
    root_spans: usize,
    cross_service_parents: usize,
    traces_across_tracer_payloads: usize,
    multi_service_chunks: usize,
    max_chunks_per_tracer_payload: usize,
    error_spans: usize,
    trace_id_high_tags: usize,
    span_links: usize,
    span_events: usize,
    idx_tracer_payloads: usize,
}

impl Coverage {
    fn new(payloads: &[AgentPayload]) -> Self {
        let mut coverage = Self::default();
        let mut tracer_payloads_by_trace = BTreeMap::<u64, BTreeSet<usize>>::new();
        let mut tracers_by_trace = BTreeMap::<u64, BTreeSet<&str>>::new();
        let mut checkout_traces = BTreeSet::new();
        let mut services_by_span = BTreeMap::new();
        let mut parents = Vec::new();
        coverage.idx_tracer_payloads = payloads
            .iter()
            .map(|payload| payload.idx_tracer_payloads.len())
            .sum();
        let tracers = payloads.iter().flat_map(|payload| &payload.tracer_payloads);
        for (index, tracer) in tracers.enumerate() {
            coverage.max_chunks_per_tracer_payload = coverage
                .max_chunks_per_tracer_payload
                .max(tracer.chunks.len());
            for chunk in &tracer.chunks {
                *coverage.priorities.entry(chunk.priority).or_default() += 1;
                let services: BTreeSet<_> = chunk.spans.iter().map(|span| &span.service).collect();
                coverage.multi_service_chunks += usize::from(services.len() > 1);
                for span in &chunk.spans {
                    *coverage
                        .tracers
                        .entry(tracer.language_name.clone())
                        .or_default() += 1;
                    *coverage.services.entry(span.service.clone()).or_default() += 1;
                    tracer_payloads_by_trace
                        .entry(span.trace_id)
                        .or_default()
                        .insert(index);
                    tracers_by_trace
                        .entry(span.trace_id)
                        .or_default()
                        .insert(&tracer.language_name);
                    services_by_span.insert((span.trace_id, span.span_id), &span.service);
                    if span.parent_id == 0 {
                        coverage.root_spans += 1;
                        if span.resource == CHECKOUT_RESOURCE {
                            checkout_traces.insert(span.trace_id);
                        }
                    } else {
                        parents.push((span.trace_id, span.parent_id, &span.service));
                    }
                    coverage.error_spans +=
                        usize::from(span.error != 0 && span.meta.contains_key("error.message"));
                    coverage.trace_id_high_tags += usize::from(span.meta.contains_key("_dd.p.tid"));
                    coverage.span_links += span.span_links.len();
                    coverage.span_events += span.span_events.len();
                }
            }
        }
        coverage.spans = coverage.services.values().sum();
        coverage.traces = tracer_payloads_by_trace.len();
        coverage.checkouts = checkout_traces.len();
        coverage.complete_checkouts = checkout_traces
            .iter()
            .filter(|trace_id| tracers_by_trace[*trace_id].len() == REQUIRED_TRACERS.len())
            .count();
        coverage.traces_across_tracer_payloads = tracer_payloads_by_trace
            .values()
            .filter(|tracer_payloads| tracer_payloads.len() > 1)
            .count();
        coverage.cross_service_parents = parents
            .into_iter()
            .filter(|(trace_id, parent_id, service)| {
                services_by_span
                    .get(&(*trace_id, *parent_id))
                    .is_some_and(|parent_service| parent_service != service)
            })
            .count();
        coverage
    }

    fn missing(&self) -> Vec<String> {
        let mut missing = Vec::new();
        for tracer in REQUIRED_TRACERS {
            if !self.tracers.contains_key(*tracer) {
                missing.push(format!("Missing tracer: {tracer}"));
            }
        }
        for service in REQUIRED_SERVICES {
            if !self.services.contains_key(*service) {
                missing.push(format!("Missing service: {service}"));
            }
        }
        // A tracer that failed to flush would otherwise only reduce coverage.
        if self.checkouts != CHECKOUTS || self.complete_checkouts != CHECKOUTS {
            missing.push(format!(
                "Expected {CHECKOUTS} checkout traces with spans from every tracer, found {} ({} complete)",
                self.checkouts, self.complete_checkouts
            ));
        }
        // Auto keep and user keep.
        for priority in [1, 2] {
            if !self.priorities.contains_key(&priority) {
                missing.push(format!("Missing sampling priority: {priority}"));
            }
        }
        for (shape, count) in [
            ("root_spans", self.root_spans),
            ("cross_service_parents", self.cross_service_parents),
            (
                "traces_across_tracer_payloads",
                self.traces_across_tracer_payloads,
            ),
            ("multi_service_chunks", self.multi_service_chunks),
            ("error_spans", self.error_spans),
            ("trace_id_high_tags", self.trace_id_high_tags),
            ("span_links", self.span_links),
            ("span_events", self.span_events),
        ] {
            if count == 0 {
                missing.push(format!("Missing trace shape: {shape}"));
            }
        }
        if self.max_chunks_per_tracer_payload < 2 {
            missing.push("Missing trace shape: multiple chunks in a tracer payload".into());
        }
        missing
    }
}

fn write(directory: &Path, name: &str, contents: &str) -> Result<(), String> {
    fs::write(directory.join(name), contents).map_err(|error| format!("{name}: {error}"))
}

fn validate_captures(
    directory: &Path,
    input: &[AgentPayload],
    output: &[AgentPayload],
) -> Result<(), String> {
    if !directory.join("capture-complete").exists() {
        return Err("Captures were not finalized; run cargo vdev e2e run datadog-traces".into());
    }
    write(directory, "input.txt", &format!("{input:#?}"))?;
    write(directory, "output.txt", &format!("{output:#?}"))?;
    let coverage = Coverage::new(input);
    let coverage_json = serde_json::to_string_pretty(&coverage).unwrap();
    write(directory, "coverage.json", &coverage_json)?;
    let differences = compare(input, output);
    write(directory, "differences.txt", &differences.join("\n\n"))?;

    let mut failures = Vec::new();
    // The source discards indexed payloads, which then also show up as missing coverage.
    if coverage.idx_tracer_payloads > 0 {
        failures.push(format!(
            "The Agent sent {} indexed tracer payloads; check `disable-convert-traces` and tracer protocols",
            coverage.idx_tracer_payloads
        ));
    }
    failures.extend(coverage.missing());
    if !differences.is_empty() {
        failures.push(format!(
            "{} differing span identities; see differences.txt",
            differences.len()
        ));
    }
    let result = if failures.is_empty() {
        "PASS: every span reached the output exactly once, unchanged.".to_string()
    } else {
        failures.join("\n")
    };
    write(
        directory,
        "summary.md",
        &format!("# Datadog Agent trace validation\n\n```json\n{coverage_json}\n```\n\n{result}\n"),
    )?;
    if failures.is_empty() {
        return Ok(());
    }
    for difference in differences.iter().take(LOGGED_DIFFERENCES) {
        failures.push(truncate(difference, LOGGED_DIFFERENCE_BYTES));
    }
    Err(failures.join("\n\n"))
}

fn truncate(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let end = text.floor_char_boundary(max_bytes);
    let kept = text.get(..end).unwrap_or_default();
    format!("{kept}\n... ({} more bytes)", text.len() - end)
}

async fn captured_payloads(base: &str) -> Vec<AgentPayload> {
    let response = get_fakeintake_payloads::<FakeIntakeResponseRaw>(base, TRACES_ENDPOINT).await;
    decode_payloads(&response).unwrap_or_else(|error| panic!("{base}: {error}"))
}

#[tokio::test]
async fn trace_relay() {
    vector::test_util::trace_init();
    let input = captured_payloads(&fake_intake_agent_address()).await;
    let output = captured_payloads(&fake_intake_vector_address()).await;
    validate_captures(Path::new(OUTPUT), &input, &output).unwrap_or_else(|error| panic!("{error}"));
}

#[cfg(test)]
mod tests {
    use datadog_proto::trace::{SpanEvent, SpanLink, TraceChunk, TracerPayload};

    use super::*;

    fn span(service: &str, trace_id: u64, span_id: u64, parent_id: u64) -> Span {
        Span {
            service: service.into(),
            trace_id,
            span_id,
            parent_id,
            ..Default::default()
        }
    }

    fn tracer(language: &str, chunks: Vec<Vec<Span>>) -> TracerPayload {
        TracerPayload {
            language_name: language.into(),
            chunks: chunks
                .into_iter()
                .enumerate()
                .map(|(index, spans)| TraceChunk {
                    // Alternate auto keep and user keep.
                    priority: if index % 2 == 0 { 1 } else { 2 },
                    spans,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    /// Checkout traces covering every required tracer, service and shape.
    fn covered() -> Vec<AgentPayload> {
        let (mut python, mut nodejs, mut go) = (Vec::new(), Vec::new(), Vec::new());
        for trace_id in 1..=u64::try_from(CHECKOUTS).unwrap() {
            let mut root = span("frontend", trace_id, 1, 0);
            root.resource = CHECKOUT_RESOURCE.into();
            root.meta
                .insert("_dd.p.tid".into(), "6a1b2c3d00000000".into());
            root.span_links.push(SpanLink::default());
            let mut failed = span("frontend-cache", trace_id, 2, 1);
            failed.error = 1;
            failed.meta.insert("error.message".into(), "failed".into());
            failed.span_events.push(SpanEvent::default());
            python.push(vec![root, failed]);
            python.push(vec![span("frontend", trace_id, 6, 1)]);
            nodejs.push(vec![span("pricing", trace_id, 3, 1)]);
            go.push(vec![
                span("inventory", trace_id, 4, 1),
                span("inventory-db", trace_id, 5, 4),
            ]);
        }
        vec![AgentPayload {
            tracer_payloads: vec![
                tracer("python", python),
                tracer("nodejs", nodejs),
                tracer("go", go),
            ],
            ..Default::default()
        }]
    }

    #[test]
    fn coverage_counts_required_shapes() {
        let coverage = Coverage::new(&covered());
        assert!(coverage.missing().is_empty(), "{:?}", coverage.missing());
        assert_eq!(coverage.spans, 6 * CHECKOUTS);
        assert_eq!(coverage.traces, CHECKOUTS);
        assert_eq!(coverage.complete_checkouts, CHECKOUTS);
        assert_eq!(coverage.traces_across_tracer_payloads, CHECKOUTS);
        assert_eq!(coverage.multi_service_chunks, 2 * CHECKOUTS);
        assert_eq!(coverage.cross_service_parents, 4 * CHECKOUTS);
        assert_eq!(coverage.error_spans, CHECKOUTS);

        let mut single_tracer = covered();
        single_tracer[0].tracer_payloads.truncate(1);
        let missing = Coverage::new(&single_tracer).missing();
        for expected in [
            "Missing tracer: go",
            "Missing service: pricing",
            "Missing trace shape: traces_across_tracer_payloads",
        ] {
            assert!(missing.iter().any(|m| m == expected), "{missing:?}");
        }
    }

    #[test]
    fn coverage_requires_every_checkout_to_be_complete() {
        // One unflushed Node span leaves a checkout without pricing.
        let mut incomplete = covered();
        incomplete[0].tracer_payloads[1].chunks.pop();
        let missing = Coverage::new(&incomplete).missing();
        assert_eq!(
            missing,
            [format!(
                "Expected {CHECKOUTS} checkout traces with spans from every tracer, found {CHECKOUTS} ({} complete)",
                CHECKOUTS - 1
            )]
        );
    }

    fn expected_covered_output(input: &[AgentPayload]) -> Vec<AgentPayload> {
        let mut output = input.to_vec();
        for chunk in &mut output[0].tracer_payloads[0].chunks {
            if chunk.spans.len() == 2 {
                chunk.spans[1]
                    .meta
                    .insert(TRACE_ID_HIGH_TAG.into(), "6a1b2c3d00000000".into());
            }
        }
        output
    }

    fn single_chunk_payload(spans: Vec<Span>) -> Vec<AgentPayload> {
        vec![AgentPayload {
            tracer_payloads: vec![tracer("python", vec![spans])],
            ..Default::default()
        }]
    }

    #[test]
    fn comparison_ignores_grouping_but_preserves_spans_and_multiplicity() {
        let input = covered();
        let output = expected_covered_output(&input);
        let mut regrouped = AgentPayload::default();
        for tracer in &output[0].tracer_payloads {
            for chunk in &tracer.chunks {
                regrouped.tracer_payloads.push(TracerPayload {
                    chunks: vec![chunk.clone()],
                    ..tracer.clone()
                });
            }
        }
        assert_eq!(compare(&input, &[regrouped]), Vec::<String>::new());

        let mut duplicated = output.clone();
        duplicated.push(output[0].clone());
        assert_ne!(compare(&input, &duplicated), Vec::<String>::new());
        assert_ne!(compare(&input, &[]), Vec::<String>::new());

        let mut changed = output;
        changed[0].tracer_payloads[0].chunks[0].spans[0]
            .span_links
            .clear();
        assert_eq!(compare(&input, &changed).len(), 1);
    }

    #[test]
    fn truncation_keeps_utf8_boundaries() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("éé", 3), "é\n... (2 more bytes)");
    }

    #[test]
    fn finalized_captures_can_be_validated_repeatedly() {
        let directory = tempfile::tempdir().unwrap();
        let input = covered();
        let output = expected_covered_output(&input);
        assert!(validate_captures(directory.path(), &input, &output).is_err());
        fs::write(directory.path().join("capture-complete"), "").unwrap();
        assert!(validate_captures(directory.path(), &input, &output).is_ok());
        assert!(validate_captures(directory.path(), &input, &output).is_ok());

        let mut changed = output.clone();
        changed[0].tracer_payloads[0].chunks[0].spans[0].name = "changed".into();
        let error = validate_captures(directory.path(), &input, &changed).unwrap_err();
        assert!(error.contains("1 differing span identities"), "{error}");
        assert!(error.contains("name: \"changed\""), "{error}");

        let mut indexed = input;
        indexed[0]
            .idx_tracer_payloads
            .push(datadog_proto::trace::idx::TracerPayload::default());
        let error = validate_captures(directory.path(), &indexed, &output).unwrap_err();
        assert!(error.contains("indexed tracer payloads"), "{error}");
    }

    #[test]
    fn comparison_allows_chunk_sibling_tid_inheritance() {
        let mut root = span("frontend", 1, 10, 0);
        root.meta
            .insert(TRACE_ID_HIGH_TAG.into(), "6b06287200000000".into());
        let child = span("frontend", 1, 11, 10);
        let input = single_chunk_payload(vec![root.clone(), child.clone()]);

        let mut normalized_child = child.clone();
        normalized_child
            .meta
            .insert(TRACE_ID_HIGH_TAG.into(), "6b06287200000000".into());
        let valid_output = single_chunk_payload(vec![root.clone(), normalized_child]);
        assert_eq!(compare(&input, &valid_output), Vec::<String>::new());

        // Legacy pre-RFC output leaves untagged child spans without _dd.p.tid.
        assert_eq!(compare(&input, &input), Vec::<String>::new());

        // Dropping _dd.p.tid from the explicitly tagged root span must still fail.
        let dropped_root_tid = single_chunk_payload(vec![span("frontend", 1, 10, 0), child]);
        assert_eq!(compare(&input, &dropped_root_tid).len(), 2);

        // Wrong _dd.p.tid on the inherited child span must fail.
        let mut wrong_child = span("frontend", 1, 11, 10);
        wrong_child
            .meta
            .insert(TRACE_ID_HIGH_TAG.into(), "1111111111111111".into());
        assert_eq!(
            compare(&input, &single_chunk_payload(vec![root, wrong_child])).len(),
            2
        );
    }

    #[test]
    fn comparison_does_not_infer_tid_across_chunks() {
        let mut chunk1_span = span("frontend", 1, 10, 0);
        chunk1_span
            .meta
            .insert(TRACE_ID_HIGH_TAG.into(), "6b06287200000000".into());
        let chunk2_span = span("frontend", 1, 11, 10);
        let input = vec![AgentPayload {
            tracer_payloads: vec![tracer(
                "python",
                vec![vec![chunk1_span.clone()], vec![chunk2_span.clone()]],
            )],
            ..Default::default()
        }];

        assert_eq!(compare(&input, &input), Vec::<String>::new());

        let mut leaked_tid = chunk2_span;
        leaked_tid
            .meta
            .insert(TRACE_ID_HIGH_TAG.into(), "6b06287200000000".into());
        let wrong_output = vec![AgentPayload {
            tracer_payloads: vec![tracer("python", vec![vec![chunk1_span], vec![leaked_tid]])],
            ..Default::default()
        }];
        assert_eq!(compare(&input, &wrong_output).len(), 2);
    }

    #[test]
    fn comparison_handles_64bit_absent_empty_and_explicit_zero_tid() {
        let absent = span("frontend", 1, 10, 0);
        let mut empty = span("frontend", 2, 20, 0);
        empty.meta.insert(TRACE_ID_HIGH_TAG.into(), String::new());
        let mut explicit_zero = span("frontend", 3, 30, 0);
        explicit_zero
            .meta
            .insert(TRACE_ID_HIGH_TAG.into(), "0000000000000000".into());
        let input =
            single_chunk_payload(vec![absent.clone(), empty.clone(), explicit_zero.clone()]);

        let valid_output = single_chunk_payload(vec![
            absent,
            span("frontend", 2, 20, 0),
            span("frontend", 3, 30, 0),
        ]);
        assert_eq!(compare(&input, &valid_output), Vec::<String>::new());

        // Retaining empty or explicit-zero _dd.p.tid on egress must fail.
        assert_eq!(compare(&input, &input).len(), 2);
    }

    #[test]
    fn comparison_rejects_incorrect_or_noncanonical_high_bits() {
        let mut root = span("frontend", 1, 10, 0);
        root.meta
            .insert(TRACE_ID_HIGH_TAG.into(), "00000000000000ab".into());
        let input = single_chunk_payload(vec![root]);

        for bad_tid in ["00000000000000ac", "00000000000000AB", "ab"] {
            let mut bad = span("frontend", 1, 10, 0);
            bad.meta.insert(TRACE_ID_HIGH_TAG.into(), bad_tid.into());
            assert!(
                !compare(&input, &single_chunk_payload(vec![bad])).is_empty(),
                "expected failure for {bad_tid}"
            );
        }

        // Low 64 bits = 0 with non-zero high bits is a valid 128-bit trace ID.
        let mut zero_low = span("frontend", 0, 10, 0);
        zero_low
            .meta
            .insert(TRACE_ID_HIGH_TAG.into(), "6b06287200000000".into());
        let zero_low_input = single_chunk_payload(vec![zero_low]);
        assert_eq!(
            compare(&zero_low_input, &zero_low_input),
            Vec::<String>::new()
        );
    }

    #[test]
    fn comparison_drops_untagged_siblings_when_chunk_is_ambiguous() {
        let mut high_span = span("frontend", 1, 10, 0);
        high_span
            .meta
            .insert(TRACE_ID_HIGH_TAG.into(), "6b06287200000000".into());
        let mut zero_span = span("frontend", 1, 11, 10);
        zero_span
            .meta
            .insert(TRACE_ID_HIGH_TAG.into(), String::new());
        let untagged = span("frontend", 1, 12, 10);
        let input = single_chunk_payload(vec![high_span.clone(), zero_span, untagged.clone()]);

        let valid_output =
            single_chunk_payload(vec![high_span.clone(), span("frontend", 1, 11, 10)]);
        assert_eq!(compare(&input, &valid_output), Vec::<String>::new());

        // Retaining the ambiguous untagged sibling must fail.
        let with_untagged =
            single_chunk_payload(vec![high_span, span("frontend", 1, 11, 10), untagged]);
        assert_eq!(compare(&input, &with_untagged).len(), 1);
    }

    #[test]
    fn comparison_still_catches_unrelated_metadata_and_multiplicity_changes() {
        let mut root = span("frontend", 1, 10, 0);
        root.meta
            .insert(TRACE_ID_HIGH_TAG.into(), "6b06287200000000".into());
        let mut child = span("frontend", 1, 11, 10);
        child.meta.insert("http.method".into(), "GET".into());
        let input = single_chunk_payload(vec![root.clone(), child]);

        let mut changed_meta = span("frontend", 1, 11, 10);
        changed_meta
            .meta
            .insert(TRACE_ID_HIGH_TAG.into(), "6b06287200000000".into());
        changed_meta
            .meta
            .insert("http.method".into(), "POST".into());
        let output_changed_meta = single_chunk_payload(vec![root.clone(), changed_meta]);
        assert_eq!(compare(&input, &output_changed_meta).len(), 1);

        let mut valid_child = span("frontend", 1, 11, 10);
        valid_child
            .meta
            .insert(TRACE_ID_HIGH_TAG.into(), "6b06287200000000".into());
        valid_child.meta.insert("http.method".into(), "GET".into());

        // Missing span fails.
        assert_eq!(
            compare(&input, &single_chunk_payload(vec![root.clone()])).len(),
            1
        );
        // Duplicate span fails.
        let duplicated = single_chunk_payload(vec![root, valid_child.clone(), valid_child]);
        assert_eq!(compare(&input, &duplicated).len(), 1);
    }
}
