use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct RemoveFieldsEventProcessed;

impl InternalEvent for RemoveFieldsEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1);
    }
}

#[derive(Debug)]
pub struct RemoveFieldsFieldMissing<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for RemoveFieldsFieldMissing<'a> {
    fn emit_logs(&self) {
        error!(message = "Field did not exist.", %self.field, rate_limit_secs = 30);
    }
}
