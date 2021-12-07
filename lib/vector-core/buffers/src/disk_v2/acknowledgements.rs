use std::sync::Arc;

use crate::Acker;

use super::Ledger;

pub(super) fn create_disk_v2_acker(ledger: Arc<Ledger>) -> Acker {
    Acker::segmented(move |amount: usize| {
        ledger.increment_pending_acks(amount);
        ledger.notify_writer_waiters();
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn ack_updates_ledger_correctly() {
        todo!();
    }

    #[test]
    fn ack_wakes_reader() {
        todo!();
    }
}
