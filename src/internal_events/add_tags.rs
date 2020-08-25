use super::InternalEvent;
use metrics::counter;

define_events_processed!(AddTagsEventProcessed, "transform", "add_tags");

#[derive(Debug)]
pub struct AddTagsTagOverwritten<'a> {
    pub tag: &'a str,
}

impl<'a> InternalEvent for AddTagsTagOverwritten<'a> {
    fn emit_logs(&self) {
        error!(message = "Tag overwritten.", %self.tag, rate_limit_secs = 30);
    }
}

#[derive(Debug)]
pub struct AddTagsTagNotOverwritten<'a> {
    pub tag: &'a str,
}

impl<'a> InternalEvent for AddTagsTagNotOverwritten<'a> {
    fn emit_logs(&self) {
        error!(message = "Tag not overwritten.", %self.tag, rate_limit_secs = 30);
    }
}
