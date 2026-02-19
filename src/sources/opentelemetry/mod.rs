#[cfg(all(test, feature = "opentelemetry-integration-tests"))]
mod integration_tests;
#[cfg(test)]
mod tests;

pub mod config;
mod grpc;
mod http;
mod reply;
mod status;

use vector_lib::{
    event::Event,
    opentelemetry::proto::{
        RESOURCE_LOGS_JSON_FIELD, RESOURCE_METRICS_JSON_FIELD, RESOURCE_SPANS_JSON_FIELD,
    },
};
use vrl::value::Value;

fn count_items_inner(resource: &Value, array_id: &str, inner_id: &str) -> usize {
    let Some(resource_array) = resource.as_array() else {
        return 0;
    };

    resource_array
        .iter()
        .map(|r| {
            r.get(array_id)
                .and_then(|s| s.as_array())
                .map(|scope_array| {
                    scope_array
                        .iter()
                        .map(|sl| {
                            sl.get(inner_id)
                                .and_then(|lr| lr.as_array())
                                .map(|arr| arr.len())
                                .unwrap_or(0)
                        })
                        .sum::<usize>()
                })
                .unwrap_or(0)
        })
        .sum()
}

/// Counts individual log records, metrics, or spans within OTLP batch events.
/// When use_otlp_decoding is enabled, events contain entire OTLP batches, but
/// we want to count the individual items for metric consistency with other sources.
/// This iterates through the Value structure, which is less efficient than
/// counting from the typed protobuf request, but avoids decoding twice.
pub(crate) fn count_otlp_items(events: &[Event]) -> usize {
    events
        .iter()
        .map(|event| match event {
            Event::Log(log) => {
                if let Some(resource_logs) = log.get(RESOURCE_LOGS_JSON_FIELD) {
                    count_items_inner(resource_logs, "scopeLogs", "logRecords")
                } else if let Some(resource_metrics) = log.get(RESOURCE_METRICS_JSON_FIELD) {
                    count_items_inner(resource_metrics, "scopeMetrics", "metrics")
                } else {
                    0
                }
            }
            Event::Trace(trace) => {
                // Count spans in resourceSpans
                if let Some(resource_spans) = trace.get(RESOURCE_SPANS_JSON_FIELD) {
                    count_items_inner(resource_spans, "scopeSpans", "spans")
                } else {
                    0
                }
            }
            _ => 0, // unreachable
        })
        .sum()
}
