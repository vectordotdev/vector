use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct RenameFieldsFieldDoesNotExist<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for RenameFieldsFieldDoesNotExist<'a> {
    fn emit(self) {
        warn!(message = "Field did not exist.", field = %self.field, internal_log_rate_secs = 30);
    }
}
