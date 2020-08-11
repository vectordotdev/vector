use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct SplitEventProcessed;

impl InternalEvent for SplitEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1,
            "component_kind" => "transform",
            "component_type" => "split",
        );
    }
}

#[derive(Debug)]
pub struct SplitFieldMissing<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for SplitFieldMissing<'a> {
    fn emit_logs(&self) {
        debug!(
            message = "field does not exist.",
            field = %self.field,
            rate_limit_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("fields_missing", 1,
            "component_kind" => "transform",
            "component_type" => "split",
        );
    }
}
