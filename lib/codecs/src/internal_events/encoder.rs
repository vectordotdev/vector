use metrics::counter;
use tracing::error;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct GelfSerializeFailedMissingField<'a> {
    pub message: &'a str,
    pub field: &'a str,
}

impl<'a> InternalEvent for GelfSerializeFailedMissingField<'a> {
    fn emit(self) {
        error!(
            message = self.message,
            field = self.field,
            internal_log_rate_secs = 10
        );
        counter!("gelf_encoder_missing_field_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct GelfSerializeFailedInvalidType<'a> {
    pub message: &'a str,
    pub field: &'a str,
    pub expected_type: &'a str,
    pub actual_type: &'a str,
}

impl<'a> InternalEvent for GelfSerializeFailedInvalidType<'a> {
    fn emit(self) {
        error!(
            message = self.message,
            field = self.field,
            expected_type = self.expected_type,
            actual_type = self.actual_type,
            internal_log_rate_secs = 10
        );
        counter!("gelf_encoder_invalid_type_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct GelfSerializeFailedInvalidFieldName<'a> {
    pub message: &'a str,
    pub field: &'a str,
}

impl<'a> InternalEvent for GelfSerializeFailedInvalidFieldName<'a> {
    fn emit(self) {
        error!(
            message = self.message,
            field = self.field,
            internal_log_rate_secs = 10
        );
        counter!("gelf_encoder_invalid_field_name_errors_total", 1);
    }
}
