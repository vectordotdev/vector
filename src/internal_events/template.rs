use metrics::counter;
use vector_core::internal_event::InternalEvent;

use super::prelude::{error_stage, error_type};

pub struct TemplateRenderingError<'a> {
    pub field: Option<&'a str>,
    pub drop_event: bool,
    pub error: crate::template::TemplateRenderingError,
}

impl<'a> InternalEvent for TemplateRenderingError<'a> {
    fn emit(self) {
        let mut msg = "Failed to render template".to_owned();
        if let Some(field) = self.field {
            use std::fmt::Write;
            let _ = write!(msg, " for \"{}\"", field);
        }
        msg.push('.');
        if self.drop_event {
            error!(
                message = "Events dropped.",
                count = 1,
                error = %self.error,
                error_type = error_type::TEMPLATE_FAILED,
                intentional = "false",
                reason = %msg,
                stage = error_stage::PROCESSING,
            );
        } else {
            error!(
                message = %msg,
                error = %self.error,
                error_type = error_type::TEMPLATE_FAILED,
                stage = error_stage::PROCESSING,
            )
        }
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
                "intentional" => "false",
                "stage" => error_stage::PROCESSING,
            );
            // deprecated
            counter!("events_discarded_total", 1);
        }
    }
}
