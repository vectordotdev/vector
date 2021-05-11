use futures::task::AtomicWaker;
use metrics::counter;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum Acker {
    Disk(Arc<AtomicUsize>, Arc<AtomicWaker>),
    Null,
}

impl Acker {
    // This method should be called by a sink to indicate that it has
    // successfully flushed the next `num` events from its input stream. If
    // there are events that have flushed, but events that came before them in
    // the stream have not been flushed, the later events must _not_ be acked
    // until all preceding elements are also acked.  This is primary used by the
    // on-disk buffer to know which events are okay to delete from disk.
    pub fn ack(&self, num: usize) {
        // Only ack items if the amount to ack is larger than zero.
        if num > 0 {
            match self {
                Acker::Null => {}
                Acker::Disk(counter, notifier) => {
                    counter.fetch_add(num, Ordering::Relaxed);
                    notifier.wake();
                }
            }

            // WARN this string "events_out_total" is a duplicate of the metric
            // name in `ROOT/src/internal_events/topology.rs`. `Acker` had a
            // dependency relationship with vector's root `internal_events`
            // prior to being migrated into core. Ideally we would not duplicate
            // information like this inside the project but I could think of no
            // other way to break the dependency in the context of PR #7400. It
            // is possible to thread this needle but the changes are more
            // substantial than one movement PR could bear.
            counter!("events_out_total", num as u64);
        }
    }
}
