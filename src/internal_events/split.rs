use super::InternalEvent;
use metrics::counter;

pub struct SplitEventProcessed;

impl InternalEvent for SplitEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1,
            "component_kind" => "transform",
            "component_type" => "split",
        );
    }
}
