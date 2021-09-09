use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct EventIn;

impl InternalEvent for EventIn {
    fn emit_metrics(&self) {
        counter!("received_events_total", 1);
    }
}

#[derive(Debug)]
pub struct EventOut {
    pub count: usize,
}

impl InternalEvent for EventOut {
    fn emit_metrics(&self) {
        if self.count > 0 {
            // WARN this string "sent_events_total" is duplicated in
            // `vector-core` as a part of PR #7400. Before you change it please
            // examine vector-core and determine if this duplication is still
            // present and if it is change that site as well. Apologies for the
            // jank.
            counter!("sent_events_total", self.count as u64);
        }
    }
}
