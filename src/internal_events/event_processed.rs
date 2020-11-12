use crate::internal_events::InternalEvent;
use metrics::counter;

/// A metric to denote the number of events processed by a topology component.
/// This is wired up already for transforms in the `topology::builder::build_pieces` function,
/// so you don't need to do that yourself.
#[derive(Debug)]
pub struct EventProcessed;

impl InternalEvent for EventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed_total", 1);
    }
}
