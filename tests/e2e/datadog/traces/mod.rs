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

/// Wire `Span.traceID` (low 64 bits) and `Span.spanID`.
type SpanKey = (u64, u64);

/// Every span with its occurrences, so missing, extra and duplicate spans all differ.
/// Payload, tracer payload and chunk grouping are ignored.
fn spans(payloads: &[AgentPayload]) -> BTreeMap<SpanKey, Vec<Vec<u8>>> {
    let mut spans: BTreeMap<SpanKey, Vec<Vec<u8>>> = BTreeMap::new();
    for span in payloads
        .iter()
        .flat_map(|payload| &payload.tracer_payloads)
        .flat_map(|tracer| &tracer.chunks)
        .flat_map(|chunk| &chunk.spans)
    {
        spans
            .entry((span.trace_id, span.span_id))
            .or_default()
            .push(span.encode_to_vec());
    }
    for occurrences in spans.values_mut() {
        occurrences.sort();
    }
    spans
}

fn compare(input: &[AgentPayload], output: &[AgentPayload]) -> Vec<String> {
    let input = spans(input);
    let output = spans(output);
    let keys: BTreeSet<_> = input.keys().chain(output.keys()).collect();
    keys.into_iter()
        .filter_map(|key| {
            let expected = input.get(key).cloned().unwrap_or_default();
            let actual = output.get(key).cloned().unwrap_or_default();
            if expected == actual {
                return None;
            }
            let decode = |spans: Vec<Vec<u8>>| {
                spans
                    .iter()
                    .map(|span| Span::decode(span.as_slice()).unwrap())
                    .collect::<Vec<_>>()
            };
            Some(format!(
                "trace={:016x} span={:016x}\nexpected: {:#?}\nactual: {:#?}",
                key.0,
                key.1,
                decode(expected),
                decode(actual),
            ))
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
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
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

    #[test]
    fn comparison_ignores_grouping_but_preserves_spans_and_multiplicity() {
        let input = covered();
        let mut regrouped = AgentPayload::default();
        for tracer in &input[0].tracer_payloads {
            for chunk in &tracer.chunks {
                regrouped.tracer_payloads.push(TracerPayload {
                    chunks: vec![chunk.clone()],
                    ..tracer.clone()
                });
            }
        }
        assert!(compare(&input, &[regrouped]).is_empty());

        let mut duplicated = input.clone();
        duplicated.push(input[0].clone());
        assert!(!compare(&input, &duplicated).is_empty());
        assert!(!compare(&input, &[]).is_empty());

        let mut changed = input.clone();
        changed[0].tracer_payloads[0].chunks[0].spans[0]
            .span_links
            .clear();
        assert_eq!(compare(&input, &changed).len(), 1);
    }

    #[test]
    fn finalized_captures_can_be_validated_repeatedly() {
        let directory = tempfile::tempdir().unwrap();
        let input = covered();
        assert!(validate_captures(directory.path(), &input, &input).is_err());
        fs::write(directory.path().join("capture-complete"), "").unwrap();
        assert!(validate_captures(directory.path(), &input, &input).is_ok());
        assert!(validate_captures(directory.path(), &input, &input).is_ok());

        let mut indexed = input.clone();
        indexed[0]
            .idx_tracer_payloads
            .push(datadog_proto::trace::idx::TracerPayload::default());
        let error = validate_captures(directory.path(), &indexed, &input).unwrap_err();
        assert!(error.contains("indexed tracer payloads"), "{error}");
    }
}
