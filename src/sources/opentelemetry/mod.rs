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

/// Counts individual log records, metrics, or spans within OTLP batch events.
/// When use_otlp_decoding is enabled, events contain entire OTLP batches, but
/// we want to count the individual items for metric consistency with other sources.
pub fn count_otlp_items(events: &[Event]) -> usize {
    events
        .iter()
        .map(|event| {
            match event {
                Event::Log(log) => {
                    // Count log records in resourceLogs
                    if let Some(resource_logs) = log.get(RESOURCE_LOGS_JSON_FIELD) {
                        if let Some(resource_logs_array) = resource_logs.as_array() {
                            return resource_logs_array
                                .iter()
                                .map(|rl| {
                                    if let Some(scope_logs) = rl.get("scopeLogs")
                                        && let Some(scope_logs_array) = scope_logs.as_array()
                                    {
                                        return scope_logs_array
                                            .iter()
                                            .map(|sl| {
                                                sl.get("logRecords")
                                                    .and_then(|lr| lr.as_array())
                                                    .map(|arr| arr.len())
                                                    .unwrap_or(0)
                                            })
                                            .sum();
                                    }
                                    0
                                })
                                .sum();
                        }
                    }
                    // Count metrics in resourceMetrics
                    else if let Some(resource_metrics) = log.get(RESOURCE_METRICS_JSON_FIELD)
                        && let Some(resource_metrics_array) = resource_metrics.as_array()
                    {
                        return resource_metrics_array
                            .iter()
                            .map(|rm| {
                                if let Some(scope_metrics) = rm.get("scopeMetrics")
                                    && let Some(scope_metrics_array) = scope_metrics.as_array()
                                {
                                    return scope_metrics_array
                                        .iter()
                                        .map(|sm| {
                                            sm.get("metrics")
                                                .and_then(|m| m.as_array())
                                                .map(|arr| arr.len())
                                                .unwrap_or(0)
                                        })
                                        .sum();
                                }
                                0
                            })
                            .sum();
                    }
                    0
                }
                Event::Trace(trace) => {
                    // Count spans in resourceSpans
                    if let Some(resource_spans) = trace.get(RESOURCE_SPANS_JSON_FIELD)
                        && let Some(resource_spans_array) = resource_spans.as_array()
                    {
                        return resource_spans_array
                            .iter()
                            .map(|rs| {
                                if let Some(scope_spans) = rs.get("scopeSpans")
                                    && let Some(scope_spans_array) = scope_spans.as_array()
                                {
                                    return scope_spans_array
                                        .iter()
                                        .map(|ss| {
                                            ss.get("spans")
                                                .and_then(|s| s.as_array())
                                                .map(|arr| arr.len())
                                                .unwrap_or(0)
                                        })
                                        .sum();
                                }
                                0
                            })
                            .sum();
                    }
                    0
                }
                _ => 0,
            }
        })
        .sum()
}
