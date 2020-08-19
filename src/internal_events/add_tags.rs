use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct AddTagsEventProcessed;

impl InternalEvent for AddTagsEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1,
            "component_kind" => "transform",
            "component_type" => "add_tags",
        );
    }
}

#[derive(Debug)]
pub struct AddTagsTagOverwritten {
    pub tag: string_cache::atom::DefaultAtom,
}

impl InternalEvent for AddTagsTagOverwritten {
    fn emit_logs(&self) {
        error!(message = "Tag overwritten.", %self.tag, rate_limit_secs = 30);
    }
}

#[derive(Debug)]
pub struct AddTagsTagNotOverwritten {
    pub tag: string_cache::atom::DefaultAtom,
}

impl InternalEvent for AddTagsTagNotOverwritten {
    fn emit_logs(&self) {
        error!(message = "Tag not overwritten.", %self.tag, rate_limit_secs = 30);
    }
}
