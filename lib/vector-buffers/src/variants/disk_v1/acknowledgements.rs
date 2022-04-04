use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use tokio::sync::Notify;

use crate::Acker;

pub fn create_disk_v1_acker(ack_counter: &Arc<AtomicUsize>, read_waker: &Arc<Notify>) -> Acker {
    let counter = Arc::clone(ack_counter);
    let notifier = Arc::clone(read_waker);

    Acker::segmented(move |amount: usize| {
        counter.fetch_add(amount, Ordering::Relaxed);
        notifier.notify_one();
    })
}
