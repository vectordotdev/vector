use std::sync::Arc;

use super::{Filesystem, Ledger};
use crate::Acker;

pub(super) fn create_disk_v2_acker<FS>(ledger: Arc<Ledger<FS>>) -> Acker
where
    FS: Filesystem + 'static,
{
    Acker::segmented(move |amount: usize| {
        ledger.increment_pending_acks(amount as u64);
        ledger.notify_writer_waiters();
    })
}
