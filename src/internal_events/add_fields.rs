use super::InternalEvent;
use crate::template::TemplateRenderingError;
use metrics::counter;

#[derive(Debug)]
pub struct AddFieldsTemplateRenderingError<'a> {
    pub field: &'a str,
    pub error: TemplateRenderingError,
}

impl<'a> InternalEvent for AddFieldsTemplateRenderingError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Failed to render templated value; discarding value.",
            field = %self.field,
            error = %self.error,
            internal_log_rate_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1);
        counter!("template_rendering_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct AddFieldsFieldOverwritten<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for AddFieldsFieldOverwritten<'a> {
    fn emit_logs(&self) {
        debug!(message = "Field overwritten.", field = %self.field, internal_log_rate_secs = 30);
    }
}

#[derive(Debug)]
pub struct AddFieldsFieldNotOverwritten<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for AddFieldsFieldNotOverwritten<'a> {
    fn emit_logs(&self) {
        debug!(message = "Field not overwritten.", field = %self.field, internal_log_rate_secs = 30);
    }
}
