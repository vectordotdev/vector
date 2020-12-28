use super::InternalEvent;

#[derive(Debug)]
pub struct RenameFieldsFieldOverwritten<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for RenameFieldsFieldOverwritten<'a> {
    fn emit_logs(&self) {
        debug!(message = "Field overwritten.", field = %self.field, rate_limit_secs = 30);
    }
}

#[derive(Debug)]
pub struct RenameFieldsFieldDoesNotExist<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for RenameFieldsFieldDoesNotExist<'a> {
    fn emit_logs(&self) {
        warn!(message = "Field did not exist.", field = %self.field, rate_limit_secs = 30);
    }
}
