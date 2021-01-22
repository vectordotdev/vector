use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct RouteEventDiscarded;

impl InternalEvent for RouteEventDiscarded {
    fn emit_metrics(&self) {
        counter!("events_discarded_total", 1);
    }
}
