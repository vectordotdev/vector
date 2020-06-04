//! A delayed delete logic.

use super::state;
use k8s_openapi::{apimachinery::pkg::apis::meta::v1::ObjectMeta, Metadata};
use std::{collections::VecDeque, time::Duration};
use tokio::time::Instant;

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
    pub async fn perform(&mut self, state_writer: &mut impl state::Write<Item = T>)
    where
        T: Metadata<Ty = ObjectMeta> + Send,
    {
        let now = Instant::now();
        while let Some(deadline) = self.next_deadline() {
            trace!(message = "got delayed deletion deadline", ?deadline, ?now);
            if deadline > now {
                break;
            }
            trace!(
                message = "processing delayed deletion for deadline",
                ?deadline,
                ?now
            );
            let (item, _) = self.queue.pop_front().unwrap();
            state_writer.delete(item).await;
        }
    }

    /// Obtain the next deadline.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.queue.front().map(|(_, instant)| *instant)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util;
    use futures::{channel::mpsc::channel, SinkExt, StreamExt};
    use k8s_openapi::api::core::v1::Pod;

    #[test]
    fn logic() {
        test_util::trace_init();
        test_util::block_on_std(async move {
            tokio::time::pause();

            let (state_events_tx, mut state_events_rx) = channel(0);
            let (mut state_actions_tx, state_actions_rx) = channel(0);
            let mut writer = state::mock::Writer::<Pod>::new(state_events_tx, state_actions_rx);

            let mut delayed_delete = DelayedDelete::new(Duration::from_millis(10000));

            {
                assert!(delayed_delete.next_deadline().is_none());
                delayed_delete.perform(&mut writer).await;
                assert!(delayed_delete.next_deadline().is_none());
                assert!(state_events_rx.try_next().is_err());
            }

            delayed_delete.schedule_delete(Pod::default());

            {
                assert!(delayed_delete.next_deadline().is_some());
                delayed_delete.perform(&mut writer).await;
                assert!(delayed_delete.next_deadline().is_some());
                assert!(state_events_rx.try_next().is_err());
            }

            tokio::time::advance(Duration::from_millis(50000)).await;

            let (mut state_events_rx, _state_actions_tx) = {
                let conc_fut = tokio::spawn(async move {
                    assert_eq!(
                        state_events_rx.next().await.unwrap().unwrap_op(),
                        (Pod::default(), state::mock::OpKind::Delete)
                    );
                    state_actions_tx.send(()).await.unwrap();
                    (state_events_rx, state_actions_tx)
                });

                assert!(delayed_delete.next_deadline().is_some());
                delayed_delete.perform(&mut writer).await;
                assert!(delayed_delete.next_deadline().is_none());

                conc_fut.await.unwrap()
            };

            {
                assert!(delayed_delete.next_deadline().is_none());
                delayed_delete.perform(&mut writer).await;
                assert!(delayed_delete.next_deadline().is_none());
                assert!(state_events_rx.try_next().is_err());
            }

            tokio::time::resume();
        });
    }
}
