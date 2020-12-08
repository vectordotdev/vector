use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct RemoveTagsEventProcessed;

impl InternalEvent for RemoveTagsEventProcessed {
    fn emit_metrics(&self) {
        counter!("processed_events_total", 1);
    }
}
