use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct SwimlanesEventProcessed;

impl InternalEvent for SwimlanesEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1,
            "component_kind" => "transform",
            "component_type" => "swimlanes",
        );
    }
}

#[derive(Debug)]
pub struct SwimlanesEventDiscarded;

impl InternalEvent for SwimlanesEventDiscarded {
    fn emit_metrics(&self) {
        counter!("events_discarded", 1,
            "component_kind" => "transform",
            "component_type" => "swimlanes",
        );
    }
}
