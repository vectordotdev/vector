//! APM stats over typed, re-coalesced chunks.
//!
//! One chunk weight, taken from the root span when that span is in the chunk, is applied
//! to every eligible span in the chunk. A service partition that joined the chunk therefore
//! sees the root `_sample_rate` even when that rate is not on its own spans.

use std::{collections::BTreeMap, sync::Arc};

use tracing::debug;

use super::aggregation::{Aggregator, PayloadAggregationKey};
use crate::sinks::datadog::traces::{dd_proto, typed::AgentGroup};

const SAMPLE_RATE_KEY: &str = "_sample_rate";
const TOP_LEVEL_KEY: &str = "_top_level";
const MEASURED_KEY: &str = "_dd.measured";
const PARTIAL_VERSION_KEY: &str = "_dd.partial_version";

/// Records stats for `groups`. `api_key` is the batch partition key.
pub(crate) fn compute_typed_apm_stats(
    api_key: Option<&Arc<str>>,
    aggregator: &std::sync::Mutex<Aggregator>,
    groups: &[AgentGroup],
) {
    let mut aggregator = aggregator.lock().expect("APM stats lock");
    for group in groups {
        aggregator.note_agent(api_key, &group.host_name, &group.env, &group.agent_version);
        for tracer in &group.tracers {
            let payload_key = PayloadAggregationKey {
                env: tracer.env.clone(),
                // Stats use the agent host (`AgentPayload.hostName`). OTLP events have no
                // agent envelope, so fall back to `Resource.host`.
                hostname: if group.host_name.is_empty() {
                    tracer.hostname.clone()
                } else {
                    group.host_name.clone()
                },
                version: tracer.app_version.clone(),
                container_id: tracer.container_id.clone(),
            };
            for chunk in &tracer.chunks {
                let weight = chunk_weight(&chunk.spans);
                let synthetics = chunk.origin.starts_with("synthetics");
                for span in &chunk.spans {
                    let (eligible, is_top) = span_stats(span);
                    if eligible {
                        aggregator.record_encoded_span(
                            span,
                            weight,
                            is_top,
                            synthetics,
                            payload_key.clone(),
                        );
                    }
                }
            }
        }
    }
}

pub(crate) fn chunk_weight(spans: &[dd_proto::Span]) -> f64 {
    if spans.is_empty() {
        return 1.0;
    }
    let mut parent_to_weight = BTreeMap::<u64, f64>::new();
    let mut span_ids = Vec::new();
    for span in spans {
        let weight = sample_weight(span.metrics.get(SAMPLE_RATE_KEY).copied());
        if span.parent_id == 0 {
            return weight;
        }
        span_ids.push(span.span_id);
        parent_to_weight.insert(span.parent_id, weight);
    }
    for id in &span_ids {
        parent_to_weight.remove(id);
    }
    if parent_to_weight.len() != 1 {
        debug!(message = "Didn't reliably find the root span for weight calculation.");
    }
    parent_to_weight.values().next().copied().unwrap_or(1.0)
}

fn sample_weight(rate: Option<f64>) -> f64 {
    match rate {
        Some(rate) if rate > 0.0 && rate <= 1.0 => 1.0 / rate,
        _ => 1.0,
    }
}

fn span_stats(span: &dd_proto::Span) -> (bool, bool) {
    let top = metric_is_one(span, TOP_LEVEL_KEY);
    let measured = metric_is_one(span, MEASURED_KEY);
    let partial = span
        .metrics
        .get(PARTIAL_VERSION_KEY)
        .is_some_and(|version| *version >= 0.0);
    (top || measured || partial, top)
}

#[allow(clippy::float_cmp, reason = "Span markers use exact values.")]
fn metric_is_one(span: &dd_proto::Span, key: &str) -> bool {
    span.metrics.get(key).is_some_and(|value| *value == 1.0)
}

#[cfg(test)]
mod tests {
    use super::chunk_weight;
    use crate::sinks::datadog::traces::dd_proto;

    fn span(parent: u64, id: u64, rate: Option<f64>) -> dd_proto::Span {
        let mut metrics = std::collections::BTreeMap::new();
        if let Some(rate) = rate {
            metrics.insert("_sample_rate".to_owned(), rate);
        }
        dd_proto::Span {
            parent_id: parent,
            span_id: id,
            metrics,
            ..Default::default()
        }
    }

    #[test]
    fn root_rate_covers_a_child_that_has_none() {
        let spans = vec![span(0, 1, Some(0.5)), span(1, 2, None)];
        assert_eq!(chunk_weight(&spans), 2.0);
    }

    #[test]
    fn missing_root_uses_the_orphan_parent_weight() {
        let spans = vec![span(9, 2, Some(0.25))];
        assert_eq!(chunk_weight(&spans), 4.0);
    }
}
