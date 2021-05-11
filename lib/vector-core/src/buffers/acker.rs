use futures::task::AtomicWaker;
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

            // TODO re-enable
            // emit!(EventOut { count: num });
        }
    }
}
