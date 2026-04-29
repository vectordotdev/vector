use std::collections::HashSet;
use std::sync::LazyLock;

use metrics::{KeyName, Label};
use metrics_tracing_context::LabelFilter;

/// A label name that should be preserved on metric keys when present as a tracing-span field.
///
/// Both Vector's own built-in global labels and downstream-registered labels go through this
/// type. Registrations are collected at link time via [`inventory`]. Downstream crates use the
/// [`register_extra_span_field!`](crate::register_extra_span_field) macro to add one.
#[derive(Debug)]
pub struct MetricLabel(pub &'static str);

inventory::collect!(MetricLabel);

// Vector's own global labels are registered through the same mechanism so the filter only has
// one allowlist to consult.
inventory::submit!(MetricLabel("component_id"));
inventory::submit!(MetricLabel("component_type"));
inventory::submit!(MetricLabel("component_kind"));
inventory::submit!(MetricLabel("buffer_type"));

/// Snapshot of every registered [`MetricLabel`], built on first access. `inventory` populates
/// all submissions before `main`, so the snapshot is guaranteed to capture every entry — the
/// hot path is then a single set lookup against this static.
static LABELS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    inventory::iter::<MetricLabel>()
        .map(|label| label.0)
        .collect()
});

#[derive(Debug, Clone)]
pub(crate) struct VectorLabelFilter;

impl LabelFilter for VectorLabelFilter {
    fn should_include_label(&self, metric_key: &KeyName, label: &Label) -> bool {
        let label_key = label.key();
        // HTTP Server-specific labels
        if metric_key.as_str().starts_with("http_server_")
            && (label_key == "method" || label_key == "path")
        {
            return true;
        }
        // gRPC Server-specific labels
        if metric_key.as_str().starts_with("grpc_server_")
            && (label_key == "grpc_method" || label_key == "grpc_service")
        {
            return true;
        }
        // Globally-registered labels: Vector's own built-ins plus any registered by downstream
        // crates via `register_extra_span_field!`.
        LABELS.contains(label_key)
    }
}

#[cfg(test)]
mod tests {
    use metrics::{KeyName, Label};
    use metrics_tracing_context::LabelFilter;

    use super::{MetricLabel, VectorLabelFilter};

    inventory::submit!(MetricLabel("test_extra_label"));

    fn key(name: &'static str) -> KeyName {
        KeyName::from_const_str(name)
    }

    #[test]
    fn includes_globally_registered_label() {
        let filter = VectorLabelFilter;
        let label = Label::new("test_extra_label", "value");
        assert!(filter.should_include_label(&key("any_metric"), &label));
    }

    #[test]
    fn excludes_unregistered_label() {
        let filter = VectorLabelFilter;
        let label = Label::new("not_registered", "value");
        assert!(!filter.should_include_label(&key("any_metric"), &label));
    }

    #[test]
    fn includes_built_in_global_labels() {
        let filter = VectorLabelFilter;
        for builtin in [
            "component_id",
            "component_type",
            "component_kind",
            "buffer_type",
        ] {
            let label = Label::new(builtin, "value");
            assert!(
                filter.should_include_label(&key("any_metric"), &label),
                "expected built-in label `{builtin}` to remain included",
            );
        }
    }
}
