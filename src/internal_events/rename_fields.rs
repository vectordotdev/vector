use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct RenameFieldsFieldOverwritten<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for RenameFieldsFieldOverwritten<'a> {
    fn emit(self) {
        debug!(message = "Field overwritten.", field = %self.field, internal_log_rate_secs = 30);
    }
}

#[derive(Debug)]
pub struct RenameFieldsFieldDoesNotExist<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for RenameFieldsFieldDoesNotExist<'a> {
    fn emit(self) {
        warn!(message = "Field did not exist.", field = %self.field, internal_log_rate_secs = 30);
    }
}
