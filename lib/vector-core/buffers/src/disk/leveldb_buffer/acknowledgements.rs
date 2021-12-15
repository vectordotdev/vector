use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use futures::task::AtomicWaker;

use crate::Acker;

pub(super) fn create_disk_v1_acker(
    ack_counter: &Arc<AtomicUsize>,
    write_notifier: &Arc<AtomicWaker>,
) -> Acker {
    let counter = Arc::clone(ack_counter);
    let notifier = Arc::clone(write_notifier);

    Acker::segmented(move |amount: usize| {
        counter.fetch_add(amount, Ordering::Relaxed);
        notifier.wake();
    })
}
