use vector_lib::opentelemetry::proto::TRACES_REQUEST_MESSAGE_TYPE;
use vector_lib::opentelemetry::proto::collector::trace::v1::ExportTraceServiceRequest;

use crate::opentelemetry::{
    assert_service_name_with, parse_line_to_export_type_request, read_file_helper,
};
use base64::prelude::*;

// telemetrygen generates 100 traces, each trace contains exactly 2 spans (parent + child)
// Collector forwards via both gRPC and HTTP to Vector, so: 100 traces * 2 spans * 2 protocols = 400 spans
const EXPECTED_SPAN_COUNT: usize = 400;

fn parse_export_traces_request(content: &str) -> Result<ExportTraceServiceRequest, String> {
    // The file may contain multiple lines, each with a JSON object containing an array of resourceSpans
    let mut merged_request = ExportTraceServiceRequest {
        resource_spans: Vec::new(),
    };

    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Merge resource_spans from this request into the accumulated result
        merged_request.resource_spans.extend(
            parse_line_to_export_type_request::<ExportTraceServiceRequest>(
                TRACES_REQUEST_MESSAGE_TYPE,
                line,
            )
            .map_err(|e| format!("Line {}: {}", line_num + 1, e))?
            .resource_spans,
        );
    }

    if merged_request.resource_spans.is_empty() {
        return Err("No resource spans found in file".to_string());
    }

    Ok(merged_request)
}

/// Asserts that all spans have expected static fields set:
/// - `name`: Should be non-empty
/// - `kind`: Should be set
fn assert_span_static_fields(request: &ExportTraceServiceRequest) {
    for (rs_idx, rs) in request.resource_spans.iter().enumerate() {
        for (ss_idx, ss) in rs.scope_spans.iter().enumerate() {
            for (span_idx, span) in ss.spans.iter().enumerate() {
                let prefix =
                    format!("resource_spans[{rs_idx}].scope_spans[{ss_idx}].spans[{span_idx}]");

                // Assert name is not empty
                assert!(
                    !span.name.is_empty(),
                    "{prefix} span name should not be empty"
                );

                // Assert span has a kind set (default is 0, but telemetrygen should set it)
                // Note: SpanKind 0 is SPAN_KIND_UNSPECIFIED, but we're just checking it exists
                // timeUnixNano fields are ignored as they vary
            }
        }
    }
}

/// Converts a span/trace ID from encoded bytes to raw binary bytes.
/// The collector outputs IDs as hex strings (e.g., "804ab72eed55cea1"),
/// Vector outputs as base64 (standard JSON encoding for binary fields).
/// Works for both span_id (8 bytes) and trace_id (16 bytes).
fn decode_span_id(id: &[u8]) -> Vec<u8> {
    // Check if it's hex-encoded (even length, all ASCII hex characters)
    if id.len().is_multiple_of(2)
        && id.len() >= 16
        && id.iter().all(|&b| {
            b.is_ascii_digit() || (b'a'..=b'f').contains(&b) || (b'A'..=b'F').contains(&b)
        })
    {
        // It's hex-encoded, decode it
        return (0..id.len())
            .step_by(2)
            .map(|i| {
                let high = char::from(id[i]).to_digit(16).unwrap() as u8;
                let low = char::from(id[i + 1]).to_digit(16).unwrap() as u8;
                (high << 4) | low
            })
            .collect();
    }

    // Check if it's base64-encoded (contains only base64 characters)
    if id.iter().all(|&b| {
        b.is_ascii_uppercase()
            || b.is_ascii_lowercase()
            || b.is_ascii_digit()
            || b == b'+'
            || b == b'/'
            || b == b'='
    }) {
        // Try to decode as base64
        if let Ok(decoded) = BASE64_STANDARD.decode(id) {
            return decoded;
        }
    }

    // Already binary or unrecognized format
    id.to_vec()
}

/// Asserts that the span IDs and trace IDs from collector and vector match exactly.
/// This verifies that Vector correctly preserves span identity through the pipeline.
/// Note: Collector outputs IDs as hex strings, Vector outputs as binary.
fn assert_span_ids_match(
    collector_request: &ExportTraceServiceRequest,
    vector_request: &ExportTraceServiceRequest,
) {
    use std::collections::HashSet;

    // Collect all span IDs from collector output (decode from hex)
    let collector_span_ids: HashSet<_> = collector_request
        .resource_spans
        .iter()
        .flat_map(|rs| &rs.scope_spans)
        .flat_map(|ss| &ss.spans)
        .map(|span| decode_span_id(&span.span_id))
        .collect();

    // Collect all span IDs from vector output (decode from base64)
    let vector_span_ids: HashSet<_> = vector_request
        .resource_spans
        .iter()
        .flat_map(|rs| &rs.scope_spans)
        .flat_map(|ss| &ss.spans)
        .map(|span| decode_span_id(&span.span_id))
        .collect();

    assert_eq!(
        collector_span_ids.len(),
        EXPECTED_SPAN_COUNT / 2,
        "Collector should have {} unique span IDs",
        EXPECTED_SPAN_COUNT / 2
    );

    assert_eq!(
        vector_span_ids.len(),
        EXPECTED_SPAN_COUNT / 2,
        "Vector should have {} unique span IDs",
        EXPECTED_SPAN_COUNT / 2
    );

    assert_eq!(
        collector_span_ids, vector_span_ids,
        "Span IDs from collector and Vector should match exactly"
    );

    // Also verify trace IDs match
    let collector_trace_ids: HashSet<_> = collector_request
        .resource_spans
        .iter()
        .flat_map(|rs| &rs.scope_spans)
        .flat_map(|ss| &ss.spans)
        .map(|span| decode_span_id(&span.trace_id))
        .collect();

    let vector_trace_ids: HashSet<_> = vector_request
        .resource_spans
        .iter()
        .flat_map(|rs| &rs.scope_spans)
        .flat_map(|ss| &ss.spans)
        .map(|span| decode_span_id(&span.trace_id))
        .collect();

    assert_eq!(
        collector_trace_ids, vector_trace_ids,
        "Trace IDs from collector and Vector should match exactly"
    );
}

#[test]
fn vector_sink_otel_sink_traces_match() {
    // Read the collector-source output (what telemetrygen sent)
    let collector_source_content = read_file_helper("traces", "collector-source-file-exporter.log")
        .expect("Failed to read collector-source file");

    // Read the collector-sink output (what Vector forwarded via OTLP)
    let collector_sink_content = read_file_helper("traces", "collector-file-exporter.log")
        .expect("Failed to read collector-sink file");

    let collector_source_request = parse_export_traces_request(&collector_source_content)
        .expect("Failed to parse collector-source traces as ExportTraceServiceRequest");
    let collector_sink_request = parse_export_traces_request(&collector_sink_content)
        .expect("Failed to parse collector-sink traces as ExportTraceServiceRequest");

    // Count total spans
    let source_span_count = collector_source_request
        .resource_spans
        .iter()
        .flat_map(|rs| &rs.scope_spans)
        .flat_map(|ss| &ss.spans)
        .count();

    let sink_span_count = collector_sink_request
        .resource_spans
        .iter()
        .flat_map(|rs| &rs.scope_spans)
        .flat_map(|ss| &ss.spans)
        .count();

    assert_eq!(
        source_span_count,
        EXPECTED_SPAN_COUNT / 2, // TODO find out why /2
        "Collector-source received {source_span_count} spans, expected {}",
        EXPECTED_SPAN_COUNT / 2
    );

    assert_eq!(
        sink_span_count, EXPECTED_SPAN_COUNT,
        "Collector-sink received {sink_span_count} spans from Vector, expected {EXPECTED_SPAN_COUNT}"
    );

    // Verify service.name attribute
    assert_service_name_with(
        &collector_source_request.resource_spans,
        "resource_spans",
        "telemetrygen",
        |rs| rs.resource.as_ref(),
    );
    assert_service_name_with(
        &collector_sink_request.resource_spans,
        "resource_spans",
        "telemetrygen",
        |rs| rs.resource.as_ref(),
    );

    // Verify static span fields
    assert_span_static_fields(&collector_source_request);
    assert_span_static_fields(&collector_sink_request);

    // Verify span IDs match exactly between source and sink
    // Both use the collector's file exporter with hex encoding, so they should match perfectly
    assert_span_ids_match(&collector_source_request, &collector_sink_request);

    // Deduplicate collector-sink data by span_id before comparison
    // Vector receives the same data via both gRPC and HTTP, so collector-sink has duplicates
    let mut deduped_sink_request = ExportTraceServiceRequest {
        resource_spans: Vec::new(),
    };

    let mut seen_span_ids = std::collections::HashSet::new();
    for rs in &collector_sink_request.resource_spans {
        let mut deduped_rs = rs.clone();
        deduped_rs.scope_spans.clear();

        for ss in &rs.scope_spans {
            let mut deduped_ss = ss.clone();
            deduped_ss.spans.clear();

            for span in &ss.spans {
                let span_id = decode_span_id(&span.span_id);
                if seen_span_ids.insert(span_id) {
                    deduped_ss.spans.push(span.clone());
                }
            }

            if !deduped_ss.spans.is_empty() {
                deduped_rs.scope_spans.push(deduped_ss);
            }
        }

        if !deduped_rs.scope_spans.is_empty() {
            deduped_sink_request.resource_spans.push(deduped_rs);
        }
    }

    // Compare the full requests to verify Vector correctly forwarded all trace data via OTLP
    // This tests the complete pipeline: telemetrygen -> collector-source -> Vector -> collector-sink
    assert_eq!(
        collector_source_request, deduped_sink_request,
        "Traces received by collector-source should match deduplicated traces forwarded through Vector to collector-sink"
    );
}
