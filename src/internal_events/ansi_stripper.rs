use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct ANSIStripperEventProcessed;

impl InternalEvent for ANSIStripperEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1,
            "component_kind" => "transform",
            "component_type" => "ansi_stripper",
        );
    }
}
