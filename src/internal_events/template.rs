use crate::emit;
use metrics::counter;
use vector_core::internal_event::{ComponentEventsDropped, InternalEvent, UNINTENTIONAL};

use vector_common::internal_event::{error_stage, error_type};

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
            _ = write!(msg, " for \"{}\"", field);
        }
        msg.push('.');

        error!(
            message = %msg,
            error = %self.error,
            error_type = error_type::TEMPLATE_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );

        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::TEMPLATE_FAILED,
            "stage" => error_stage::PROCESSING,
        );

        // deprecated
        counter!("processing_errors_total", 1,
            "error_type" => "render_error");

        if self.drop_event {
            emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                count: 1,
                reason: "Failed to render template.",
            });

            // deprecated
            counter!("events_discarded_total", 1);
        }
    }
}
