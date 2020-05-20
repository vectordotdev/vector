use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct AddFieldsEventProcessed;

impl InternalEvent for AddFieldsEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1,
            "component_kind" => "transform",
            "component_type" => "add_fields",
        );
    }
}
