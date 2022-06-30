use metrics::counter;
use tracing::error;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct SerializeFailedMissingField<'a> {
    pub format_type: &'a str,
    pub message: &'a str,
    pub field: &'a str,
}

impl<'a> InternalEvent for SerializeFailedMissingField<'a> {
    fn emit(self) {
        error!(
            message = self.message,
            format_type = self.format_type,
            field = self.field,
            internal_log_rate_secs = 10
        );
        counter!("encoder_missing_field_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct SerializeFailedInvalidType<'a> {
    pub format_type: &'a str,
    pub message: &'a str,
    pub field: &'a str,
    pub expected_type: &'a str,
    pub actual_type: &'a str,
}

impl<'a> InternalEvent for SerializeFailedInvalidType<'a> {
    fn emit(self) {
        error!(
            message = self.message,
            format_type = self.format_type,
            field = self.field,
            expected_type = self.expected_type,
            actual_type = self.actual_type,
            internal_log_rate_secs = 10
        );
        counter!("encoder_invalid_type_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct SerializeFailedInvalidFieldName<'a> {
    pub format_type: &'a str,
    pub message: &'a str,
    pub field: &'a str,
}

impl<'a> InternalEvent for SerializeFailedInvalidFieldName<'a> {
    fn emit(self) {
        error!(
            message = self.message,
            format_type = self.format_type,
            field = self.field,
            internal_log_rate_secs = 10
        );
        counter!("encoder_invalid_field_name_errors_total", 1);
    }
}
