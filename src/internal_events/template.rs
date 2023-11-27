use metrics::counter;
use vector_lib::internal_event::{error_stage, error_type};
use vector_lib::internal_event::{ComponentEventsDropped, InternalEvent, UNINTENTIONAL};

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

        if self.drop_event {
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

            emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                count: 1,
                reason: "Failed to render template.",
            });
        } else {
            warn!(
                message = %msg,
                error = %self.error,
                error_type = error_type::TEMPLATE_FAILED,
                stage = error_stage::PROCESSING,
                internal_log_rate_limit = true,
            );
        }
    }
}
