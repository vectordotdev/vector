use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct GeneratorEventProcessed;

impl InternalEvent for GeneratorEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1,
            "component_kind" => "source",
            "component_type" => "generator",
        );
    }
}
