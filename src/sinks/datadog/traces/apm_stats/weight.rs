use std::collections::BTreeMap;

use crate::event::{ObjectMap, Value};

const SAMPLING_RATE_KEY: &str = "_sample_rate";

/// This extracts the relative weights from the top level span (i.e. the span that does not have a parent).
pub(crate) fn extract_weight_from_root_span(spans: &[&ObjectMap]) -> f64 {
    // Based on https://github.com/DataDog/datadog-agent/blob/cfa750c7412faa98e87a015f8ee670e5828bbe7f/pkg/trace/stats/weight.go#L17-L26.

    // TODO this logic likely has a bug(s) that need to be root caused. The root span is not reliably found and defaults to "1.0"
    // regularly for users even when sampling is disabled in the Agent.
    // GH issue to track that: https://github.com/vectordotdev/vector/issues/14859

    if spans.is_empty() {
        return 1.0;
    }

    let mut trace_id: Option<usize> = None;

    let mut parent_id_to_child_weight = BTreeMap::<i64, f64>::new();
    let mut span_ids = Vec::<i64>::new();
    for s in spans.iter() {
        // TODO these need to change to u64 when the following issue is fixed:
        // https://github.com/vectordotdev/vector/issues/14687
        let parent_id = match s.get("parent_id") {
            Some(Value::Integer(val)) => *val,
            None => 0,
            _ => panic!("`parent_id` should be an i64"),
        };
        let span_id = match s.get("span_id") {
            Some(Value::Integer(val)) => *val,
            None => 0,
            _ => panic!("`span_id` should be an i64"),
        };
        if trace_id.is_none() {
            trace_id = match s.get("trace_id") {
                Some(Value::Integer(v)) => Some(*v as usize),
                _ => panic!("`trace_id` should be an i64"),
            }
        }
        let weight = s
            .get("metrics")
            .and_then(|m| m.as_object())
            .map(|m| match m.get(SAMPLING_RATE_KEY) {
                Some(Value::Float(v)) => {
                    let sample_rate = v.into_inner();
                    if sample_rate <= 0.0 || sample_rate > 1.0 {
                        1.0
                    } else {
                        1.0 / sample_rate
                    }
                }
                _ => 1.0,
            })
            .unwrap_or(1.0);

        // found root
        if parent_id == 0 {
            return weight;
        }

        span_ids.push(span_id);

        parent_id_to_child_weight.insert(parent_id, weight);
    }

    // Remove all spans that have a parent
    span_ids.iter().for_each(|id| {
        parent_id_to_child_weight.remove(id);
    });

    // There should be only one value remaining, the weight from the root span
    if parent_id_to_child_weight.len() != 1 {
        // TODO remove the debug print and emit the Error event as outlined in
        // https://github.com/vectordotdev/vector/issues/14859
        debug!(
            "Didn't reliably find the root span for weight calculation of trace_id {:?}.",
            trace_id
        );
    }

    *parent_id_to_child_weight
        .values()
        .next()
        .unwrap_or_else(|| {
            // TODO remove the debug print and emit the Error event as outlined in
            // https://github.com/vectordotdev/vector/issues/14859
            debug!(
                "Root span was not found. Defaulting to weight of 1.0 for trace_id {:?}.",
                trace_id
            );
            &1.0
        })
}
