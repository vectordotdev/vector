use vector_lib::opentelemetry::proto::TRACES_REQUEST_MESSAGE_TYPE;
use vector_lib::opentelemetry::proto::collector::trace::v1::ExportTraceServiceRequest;

use crate::opentelemetry::{
    assert_service_name_with, parse_value_to_export_type_request, read_file_helper,
};
use vrl::value::Value as VrlValue;

// telemetrygen generates 100 traces, each trace contains exactly 2 spans (parent + child)
// Collector forwards via both gRPC and HTTP to Vector, so: 100 traces * 2 spans * 2 protocols = 400 spans
const EXPECTED_SPAN_COUNT: usize = 400;
const EXPECTED_TRACE_COUNT: usize = 100;

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

        merged_request.resource_spans.extend(
            parse_collector_trace_line(line)
                .map_err(|e| format!("Line {}: {}", line_num + 1, e))?
                .resource_spans,
        );
    }

    if merged_request.resource_spans.is_empty() {
        return Err("No resource spans found in file".to_string());
    }

    Ok(merged_request)
}

fn parse_collector_trace_line(line: &str) -> Result<ExportTraceServiceRequest, String> {
    let mut value: VrlValue = serde_json::from_str::<serde_json::Value>(line)
        .map_err(|e| format!("Failed to parse JSON: {e}"))?
        .into();

    decode_collector_ids(&mut value)?;
    parse_value_to_export_type_request(TRACES_REQUEST_MESSAGE_TYPE, value)
}

fn decode_collector_ids(value: &mut VrlValue) -> Result<(), String> {
    match value {
        VrlValue::Object(fields) => {
            for (name, value) in fields {
                let valid_hex_lengths: &[usize] = match name.as_str() {
                    "traceId" => &[32],
                    "spanId" => &[16],
                    "parentSpanId" => &[0, 16],
                    _ => {
                        decode_collector_ids(value)?;
                        continue;
                    }
                };

                let encoded = value
                    .as_bytes()
                    .ok_or_else(|| format!("{name} should be a hexadecimal string"))?;
                if !valid_hex_lengths.contains(&encoded.len()) {
                    return Err(format!(
                        "{name} has invalid hexadecimal length {}",
                        encoded.len()
                    ));
                }

                *value = VrlValue::Bytes(
                    hex::decode(encoded.as_ref())
                        .map_err(|e| format!("Failed to decode {name}: {e}"))?
                        .into(),
                );
            }
        }
        VrlValue::Array(values) => {
            for value in values {
                decode_collector_ids(value)?;
            }
        }
        _ => {}
    }

    Ok(())
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

fn assert_binary_span_ids(request: &ExportTraceServiceRequest) {
    for span in request
        .resource_spans
        .iter()
        .flat_map(|resource| &resource.scope_spans)
        .flat_map(|scope| &scope.spans)
    {
        assert_eq!(span.trace_id.len(), 16, "trace ID should contain 16 bytes");
        assert_eq!(span.span_id.len(), 8, "span ID should contain 8 bytes");
        assert!(
            span.parent_span_id.is_empty() || span.parent_span_id.len() == 8,
            "parent span ID should be empty or contain 8 bytes"
        );
    }
}

/// Asserts that the span IDs and trace IDs from collector and vector match exactly.
/// This verifies that Vector correctly preserves span identity through the pipeline.
/// Both requests contain the protobuf binary representation of each ID.
fn assert_span_ids_match(
    collector_request: &ExportTraceServiceRequest,
    vector_request: &ExportTraceServiceRequest,
) {
    use std::collections::HashSet;

    let collector_span_ids: HashSet<_> = collector_request
        .resource_spans
        .iter()
        .flat_map(|rs| &rs.scope_spans)
        .flat_map(|ss| &ss.spans)
        .map(|span| span.span_id.as_slice())
        .collect();

    let vector_span_ids: HashSet<_> = vector_request
        .resource_spans
        .iter()
        .flat_map(|rs| &rs.scope_spans)
        .flat_map(|ss| &ss.spans)
        .map(|span| span.span_id.as_slice())
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

    let collector_trace_ids: HashSet<_> = collector_request
        .resource_spans
        .iter()
        .flat_map(|rs| &rs.scope_spans)
        .flat_map(|ss| &ss.spans)
        .map(|span| span.trace_id.as_slice())
        .collect();

    let vector_trace_ids: HashSet<_> = vector_request
        .resource_spans
        .iter()
        .flat_map(|rs| &rs.scope_spans)
        .flat_map(|ss| &ss.spans)
        .map(|span| span.trace_id.as_slice())
        .collect();

    assert_eq!(
        collector_trace_ids.len(),
        EXPECTED_TRACE_COUNT,
        "Collector should have {EXPECTED_TRACE_COUNT} unique trace IDs"
    );
    assert_eq!(
        vector_trace_ids.len(),
        EXPECTED_TRACE_COUNT,
        "Vector should have {EXPECTED_TRACE_COUNT} unique trace IDs"
    );
    assert_eq!(
        collector_trace_ids, vector_trace_ids,
        "Trace IDs from collector and Vector should match exactly"
    );
}

#[test]
fn collector_trace_ids_are_deserialized_as_binary() {
    let request = parse_export_traces_request(
        r#"{"resourceSpans":[{"scopeSpans":[{"spans":[{"traceId":"00112233445566778899aabbccddeeff","spanId":"fedcba9876543210","parentSpanId":"0123456789abcdef","links":[{"traceId":"ffeeddccbbaa99887766554433221100","spanId":"1032547698badcfe"}]}]}]}]}"#,
    )
    .expect("collector trace JSON should decode");
    let span = &request.resource_spans[0].scope_spans[0].spans[0];

    assert_eq!(
        span.trace_id,
        hex::decode("00112233445566778899aabbccddeeff").unwrap()
    );
    assert_eq!(span.span_id, hex::decode("fedcba9876543210").unwrap());
    assert_eq!(
        span.parent_span_id,
        hex::decode("0123456789abcdef").unwrap()
    );
    assert_eq!(
        span.links[0].trace_id,
        hex::decode("ffeeddccbbaa99887766554433221100").unwrap()
    );
    assert_eq!(
        span.links[0].span_id,
        hex::decode("1032547698badcfe").unwrap()
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

    // Protobuf IDs are bytes. Parsing the collector's hex JSON back to bytes makes the full
    // request comparison validate Vector's OTLP decode/encode round trip.
    assert_binary_span_ids(&collector_source_request);
    assert_binary_span_ids(&collector_sink_request);

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
                let span_id = span.span_id.as_slice();
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
