use std::sync::Arc;

use super::Ledger;

/// Handles acknowledgements for a disk buffer.
///
/// NOTE: This is effectively a duplicate of `buffers::Acker` so that we could avoid having to
/// change `buffers::Acker` even though disk buffers v2 aren't yet wired in.
///
/// As part of the work to actually switch over to the new buffer types/topology, we'll unify this
/// type with `buffers::Acker`.
pub struct Acker {
    ledger: Arc<Ledger>,
}

impl Acker {
    pub(super) fn from_ledger(ledger: &Arc<Ledger>) -> Self {
        Self {
            ledger: Arc::clone(ledger),
        }
    }

    /// Acknowledge a certain amount of records.
    ///
    /// Callers are responsible for ensuring that acknowledgements are in order.  That is to say, if
    /// 100 records are read from the buffer, and the 2nd to the 100th record are all durably
    /// processed, none of them can be acknowledged until the 1st record has also been durably processed.
    pub fn acknowledge_records(&self, amount: usize) {
        // We update the number of pending acknowledgements in the ledger, and make sure to notify
        // the reader since they may not be any more data to read, thus leaving them waiting for a
        // notification from what would otherwise be the writer saying "hey, wake up, you have more
        // data now".
        self.ledger.increment_pending_acks(amount);
        self.ledger.notify_writer_waiters();
    }
}
