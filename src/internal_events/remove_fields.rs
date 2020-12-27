use super::InternalEvent;

#[derive(Debug)]
pub struct RemoveFieldsFieldMissing<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for RemoveFieldsFieldMissing<'a> {
    fn emit_logs(&self) {
        debug!(message = "Field did not exist.", field = %self.field, internal_log_rate_secs = 30);
    }
}
