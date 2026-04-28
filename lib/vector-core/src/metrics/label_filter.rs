use metrics::{KeyName, Label};
use metrics_tracing_context::LabelFilter;

/// Extra label name that downstream crates can register so it is preserved on metric keys when
/// it is present as a tracing-span field.
///
/// Registration is collected at link time via [`inventory`], so reads are lock-free.
/// Use the [`register_extra_metric_label!`](crate::register_extra_metric_label) macro to
/// register a label.
#[derive(Debug)]
pub struct ExtraMetricLabel(pub &'static str);

inventory::collect!(ExtraMetricLabel);

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
        // Global labels
        if label_key == "component_id"
            || label_key == "component_type"
            || label_key == "component_kind"
            || label_key == "buffer_type"
        {
            return true;
        }
        // Extra labels registered by downstream crates.
        inventory::iter::<ExtraMetricLabel>
            .into_iter()
            .any(|extra| extra.0 == label_key)
    }
}

#[cfg(test)]
mod tests {
    use metrics::{KeyName, Label};
    use metrics_tracing_context::LabelFilter;

    use super::{ExtraMetricLabel, VectorLabelFilter};

    inventory::submit!(ExtraMetricLabel("test_extra_label"));

    fn key(name: &'static str) -> KeyName {
        KeyName::from_const_str(name)
    }

    #[test]
    fn includes_globally_registered_extra_label() {
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
    fn still_includes_built_in_global_labels() {
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
