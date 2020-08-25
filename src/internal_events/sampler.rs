use super::InternalEvent;
use metrics::counter;

define_events_processed!(SamplerEventProcessed, "transform", "sampler");

#[derive(Debug)]
pub struct SamplerEventDiscarded;

impl InternalEvent for SamplerEventDiscarded {
    fn emit_metrics(&self) {
        counter!("events_discarded", 1,
            "component_kind" => "transform",
            "component_type" => "sampler",
        );
    }
}
