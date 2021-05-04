use metrics::Label;
use metrics_tracing_context::LabelFilter;

#[derive(Debug, Clone)]
pub(crate) struct VectorLabelFilter;

impl LabelFilter for VectorLabelFilter {
    fn should_include_label(&self, label: &Label) -> bool {
        let key = label.key();
        key == "component_name" || key == "component_type" || key == "component_kind"
    }
}
