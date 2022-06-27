use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct AddFieldsFieldNotOverwritten<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for AddFieldsFieldNotOverwritten<'a> {
    fn emit(self) {
        debug!(message = "Field not overwritten.", field = %self.field, internal_log_rate_secs = 30);
    }
}
