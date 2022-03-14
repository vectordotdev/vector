use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct RouteEventDiscarded<'a> {
    pub outputs: Vec<&'a str>,
}

impl<'a> InternalEvent for RouteEventDiscarded<'a> {
    fn emit_metrics(&self) {
        let outputs = self.outputs.join(", ");
        counter!("events_discarded_total", self.outputs.len() as u64, "outputs" => outputs);
    }
}
