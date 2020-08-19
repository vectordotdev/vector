use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct AddFieldsEventProcessed;

impl InternalEvent for AddFieldsEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1,
            "component_kind" => "transform",
            "component_type" => "add_fields",
        );
    }
}

#[derive(Debug)]
pub struct AddFieldsTemplateRenderingError {
    pub field: string_cache::atom::DefaultAtom,
}

impl InternalEvent for AddFieldsTemplateRenderingError {
    fn emit_logs(&self) {
        error!(message = "Failed to render templated value; discarding value.", %self.field, rate_limit_secs = 30);
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1,
            "component_kind" => "transform",
            "component_type" => "add_fields",
        );
    }
}

#[derive(Debug)]
pub struct AddFieldsTemplateInvalid {
    pub error: crate::template::TemplateError,
    pub field: string_cache::atom::DefaultAtom,
}

impl InternalEvent for AddFieldsTemplateInvalid {
    fn emit_logs(&self) {
        error!(message = "Invalid template; using as string", %self.field, %self.error, rate_limit_secs = 30);
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1,
            "component_kind" => "transform",
            "component_type" => "add_fields",
        );
    }
}

#[derive(Debug)]
pub struct AddFieldsFieldOverwritten {
    pub field: string_cache::atom::DefaultAtom,
}

impl InternalEvent for AddFieldsFieldOverwritten {
    fn emit_logs(&self) {
        error!(message = "Field overwritten.", %self.field, rate_limit_secs = 30);
    }
}

#[derive(Debug)]
pub struct AddFieldsFieldNotOverwritten {
    pub field: string_cache::atom::DefaultAtom,
}

impl InternalEvent for AddFieldsFieldNotOverwritten {
    fn emit_logs(&self) {
        error!(message = "Field not overwritten.", %self.field, rate_limit_secs = 30);
    }
}
