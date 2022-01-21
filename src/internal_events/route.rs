use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct RouteEventDiscarded<'a> {
    pub output: &'a str,
}

impl<'a> InternalEvent for RouteEventDiscarded<'a> {
    fn emit_metrics(&self) {
        counter!("events_discarded_total", 1, "output" => self.output.to_string());
    }
}
