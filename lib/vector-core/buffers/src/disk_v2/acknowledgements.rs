use std::sync::Arc;

use super::Ledger;
use crate::Acker;

pub(super) fn create_disk_v2_acker(ledger: Arc<Ledger>) -> Acker {
    Acker::segmented(move |amount: usize| {
        ledger.increment_pending_acks(amount);
        ledger.notify_writer_waiters();
    })
}
