//! A delayed delete logic.

use super::state;
use k8s_openapi::{apimachinery::pkg::apis::meta::v1::ObjectMeta, Metadata};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

/// Implements the logic for delaying the deletion of items from the storage.
pub struct DelayedDelete<T> {
    queue: VecDeque<(T, Instant)>,
    delay_for: Duration,
}

impl<T> DelayedDelete<T> {
    /// Create a new [`DelayedDelete`] state.
    pub fn new(delay_for: Duration) -> Self {
        let queue = VecDeque::new();
        Self { queue, delay_for }
    }

    /// Schedules the delayed deletion of the item at the future.
    pub fn schedule_delete(&mut self, item: T) {
        let deadline = Instant::now() + self.delay_for;
        self.queue.push_back((item, deadline));
    }

    /// Clear the delayed deletion requests.
    pub fn clear(&mut self) {
        self.queue.clear();
    }

    /// Perform the queued deletions.
    pub fn perform(&mut self, state_writer: &mut impl state::Write<Item = T>)
    where
        T: Metadata<Ty = ObjectMeta>,
    {
        let now = Instant::now();
        while let Some(deadline) = self.next_deadline() {
            if deadline > now {
                break;
            }
            let (item, _) = self.queue.pop_front().unwrap();
            state_writer.delete(item);
        }
    }

    /// Obtain the next deadline.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.queue.front().map(|(_, instant)| *instant)
    }
}
