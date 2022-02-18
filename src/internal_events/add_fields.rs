use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub(crate) struct AddFieldsFieldOverwritten<'a> {
    pub(crate) field: &'a str,
}

impl<'a> InternalEvent for AddFieldsFieldOverwritten<'a> {
    fn emit_logs(&self) {
        debug!(message = "Field overwritten.", field = %self.field, internal_log_rate_secs = 30);
    }
}

#[derive(Debug)]
pub(crate) struct AddFieldsFieldNotOverwritten<'a> {
    pub(crate) field: &'a str,
}

impl<'a> InternalEvent for AddFieldsFieldNotOverwritten<'a> {
    fn emit_logs(&self) {
        debug!(message = "Field not overwritten.", field = %self.field, internal_log_rate_secs = 30);
    }
}
