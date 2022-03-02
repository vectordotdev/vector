use super::prelude::{error_stage, error_type};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

pub struct TemplateRenderingError<'a> {
    pub field: Option<&'a str>,
    pub drop_event: bool,
    pub error: crate::template::TemplateRenderingError,
}

impl<'a> InternalEvent for TemplateRenderingError<'a> {
    fn emit_logs(&self) {
        let mut msg = "Failed to render template".to_owned();
        if let Some(field) = self.field {
            use std::fmt::Write;
            let _ = write!(msg, " for \"{}\"", field);
        }
        if self.drop_event {
            msg.push_str("; discarding event");
        }
        msg.push('.');
        error!(
            message = %msg,
            error = %self.error,
            error_type = error_type::TEMPLATE_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 30,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::TEMPLATE_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("processing_errors_total", 1,
            "error_type" => "render_error");
        if self.drop_event {
            counter!(
                "component_discarded_events_total", 1,
                "error_type" => error_type::TEMPLATE_FAILED,
                "stage" => error_stage::PROCESSING,
            );
            // deprecated
            counter!("events_discarded_total", 1);
        }
    }
}
