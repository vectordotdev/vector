use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct RenameFieldsEventProcessed;

impl InternalEvent for RenameFieldsEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1);
    }
}

#[derive(Debug)]
pub struct RenameFieldsFieldOverwritten<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for RenameFieldsFieldOverwritten<'a> {
    fn emit_logs(&self) {
        error!(message = "Field overwritten.", %self.field, rate_limit_secs = 30);
    }
}

#[derive(Debug)]
pub struct RenameFieldsFieldDoesNotExist<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for RenameFieldsFieldDoesNotExist<'a> {
    fn emit_logs(&self) {
        error!(message = "Field did not exist.", %self.field, rate_limit_secs = 30);
    }
}
