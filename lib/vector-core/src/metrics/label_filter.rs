use metrics::{KeyName, Label};
use metrics_tracing_context::LabelFilter;

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
        label_key == "component_id"
            || label_key == "component_type"
            || label_key == "component_kind"
            || label_key == "buffer_type"
    }
}
