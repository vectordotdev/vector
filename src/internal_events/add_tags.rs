use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct AddTagsEventProcessed;

impl InternalEvent for AddTagsEventProcessed {
    fn emit_metrics(&self) {
        counter!("processed_events_total", 1);
    }
}

#[derive(Debug)]
pub struct AddTagsTagOverwritten<'a> {
    pub tag: &'a str,
}

impl<'a> InternalEvent for AddTagsTagOverwritten<'a> {
    fn emit_logs(&self) {
        debug!(message = "Tag overwritten.", tag = %self.tag, rate_limit_secs = 30);
    }
}

#[derive(Debug)]
pub struct AddTagsTagNotOverwritten<'a> {
    pub tag: &'a str,
}

impl<'a> InternalEvent for AddTagsTagNotOverwritten<'a> {
    fn emit_logs(&self) {
        debug!(message = "Tag not overwritten.", tag = %self.tag, rate_limit_secs = 30);
    }
}
