use std::sync::Arc;

use super::Ledger;

pub struct Acker {
    ledger: Arc<Ledger>,
}

impl Acker {
    pub fn from_ledger(ledger: &Arc<Ledger>) -> Self {
        Self {
            ledger: Arc::clone(ledger),
        }
    }

    pub fn acknowledge_records(&self, amount: usize) {
        // We update the number of pending acknowledgements in the ledger, and make sure to notify
        // the reader since they may not be any more data to read, thus leaving them waiting for a
        // notification from what would otherwise be the writer saying "hey, wake up, you have more
        // data now".
        self.ledger.increment_pending_acks(amount);
        self.ledger.notify_writer_waiters();
    }
}
