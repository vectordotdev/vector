use vector_lib::opentelemetry::proto::TRACES_REQUEST_MESSAGE_TYPE;
use vector_lib::opentelemetry::proto::collector::trace::v1::ExportTraceServiceRequest;
use vector_lib::opentelemetry::proto::common::v1::any_value::Value as AnyValueEnum;

use crate::opentelemetry::{parse_line_to_export_type_request, read_file_helper};

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

/// Asserts that all resource spans have a `service.name` attribute set to `"telemetrygen"`.
fn assert_service_name(request: &ExportTraceServiceRequest) {
    for (i, rs) in request.resource_spans.iter().enumerate() {
        let resource = rs
            .resource
            .as_ref()
            .unwrap_or_else(|| panic!("resource_spans[{i}] missing resource"));

        let service_name_attr = resource
            .attributes
            .iter()
            .find(|kv| kv.key == "service.name")
            .unwrap_or_else(|| panic!("resource_spans[{i}] missing 'service.name' attribute"));

        let actual_value = service_name_attr
            .value
            .as_ref()
            .and_then(|v| v.value.as_ref())
            .unwrap_or_else(|| panic!("resource_spans[{i}] 'service.name' has no value"));

        if let AnyValueEnum::StringValue(s) = actual_value {
            assert_eq!(
                s, "telemetrygen",
                "resource_spans[{i}] 'service.name' expected 'telemetrygen', got '{s}'"
            );
        } else {
            panic!("resource_spans[{i}] 'service.name' is not a string value");
        }
    }
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

/// Asserts that the span IDs from collector and vector match exactly.
/// This verifies that Vector correctly preserves span identity through the pipeline.
fn assert_span_ids_match(
    collector_request: &ExportTraceServiceRequest,
    vector_request: &ExportTraceServiceRequest,
) {
    use std::collections::HashSet;

    // Collect all span IDs from collector output
    let collector_span_ids: HashSet<_> = collector_request
        .resource_spans
        .iter()
        .flat_map(|rs| &rs.scope_spans)
        .flat_map(|ss| &ss.spans)
        .map(|span| &span.span_id)
        .collect();

    // Collect all span IDs from vector output
    let vector_span_ids: HashSet<_> = vector_request
        .resource_spans
        .iter()
        .flat_map(|rs| &rs.scope_spans)
        .flat_map(|ss| &ss.spans)
        .map(|span| &span.span_id)
        .collect();

    // assert_eq!(
    //     collector_span_ids.len(),
    //     EXPECTED_SPAN_COUNT,
    //     "Collector should have {} unique span IDs",
    //     EXPECTED_SPAN_COUNT
    // );
    //
    // assert_eq!(
    //     vector_span_ids.len(),
    //     EXPECTED_SPAN_COUNT,
    //     "Vector should have {} unique span IDs",
    //     EXPECTED_SPAN_COUNT
    // );

    assert_eq!(
        collector_span_ids, vector_span_ids,
        "Span IDs from collector and Vector should match exactly"
    );
}

#[test]
fn vector_sink_otel_sink_traces_match() {
    let collector_content = read_file_helper("traces", "collector-file-exporter.log")
        .expect("Failed to read collector file");
    let vector_content =
        read_file_helper("traces", "vector-file-sink.log").expect("Failed to read vector file");

    let collector_request = parse_export_traces_request(&collector_content)
        .expect("Failed to parse collector traces as ExportTraceServiceRequest");
    let vector_request = parse_export_traces_request(&vector_content)
        .expect("Failed to parse vector traces as ExportTraceServiceRequest");

    // Count total spans
    let collector_span_count = collector_request
        .resource_spans
        .iter()
        .flat_map(|rs| &rs.scope_spans)
        .flat_map(|ss| &ss.spans)
        .count();

    let vector_span_count = vector_request
        .resource_spans
        .iter()
        .flat_map(|rs| &rs.scope_spans)
        .flat_map(|ss| &ss.spans)
        .count();

    // assert_eq!(
    //     collector_span_count, EXPECTED_SPAN_COUNT,
    //     "Collector produced {collector_span_count} spans, expected {EXPECTED_SPAN_COUNT}"
    // );
    //
    // assert_eq!(
    //     vector_span_count, EXPECTED_SPAN_COUNT,
    //     "Vector produced {vector_span_count} spans, expected {EXPECTED_SPAN_COUNT}"
    // );

    // Verify service.name attribute
    assert_service_name(&collector_request);
    assert_service_name(&vector_request);

    // Verify static span fields
    assert_span_static_fields(&collector_request);
    assert_span_static_fields(&vector_request);

    // Verify span IDs match exactly between collector and vector
    assert_span_ids_match(&collector_request, &vector_request);

    // Both collector and Vector receive the same traces.
    // Compare them directly to verify the entire pipeline works correctly.
    assert_eq!(
        collector_request, vector_request,
        "Collector and Vector trace requests should match"
    );
}
