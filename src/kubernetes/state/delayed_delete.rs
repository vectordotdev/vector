//! A state wrapper that delays deletes.

use std::{collections::VecDeque, time::Duration};

use async_trait::async_trait;
use futures::{future::BoxFuture, FutureExt};
use tokio::time::{sleep_until, timeout_at, Instant};

/// A [`super::Write`] implementation that wraps another [`super::Write`] and
/// delays the delete calls.
/// Implements the logic for delaying the deletion of items from the storage.
pub struct Writer<T>
where
    T: super::Write + Send,
    <T as super::Write>::Item: Send + Sync,
{
    inner: T,
    queue: VecDeque<(<T as super::Write>::Item, Instant)>,
    sleep: Duration,
}

impl<T> Writer<T>
where
    T: super::Write + Send,
    <T as super::Write>::Item: Send + Sync,
{
    /// Take a [`super::Write`] and return it wrapped with [`Writer`].
    pub fn new(inner: T, sleep: Duration) -> Self {
        let queue = VecDeque::new();
        Self {
            inner,
            queue,
            sleep,
        }
    }
}

impl<T> Writer<T>
where
    T: super::Write + Send,
    <T as super::Write>::Item: Send + Sync,
{
    /// Schedules the delayed deletion of the item at the future.
    pub fn schedule_delete(&mut self, item: <T as super::Write>::Item) {
        let deadline = Instant::now() + self.sleep;
        self.queue.push_back((item, deadline));
    }

    /// Clear the delayed deletion requests.
    pub fn clear(&mut self) {
        self.queue.clear();
    }

    /// Perform the queued deletions.
    pub async fn perform(&mut self) {
        let now = Instant::now();
        while let Some((_, deadline)) = self.queue.front() {
            let deadline = *deadline;
            trace!(message = "Got delayed deletion deadline.", deadline = ?deadline, now = ?now);
            if deadline > now {
                break;
            }
            trace!(
                message = "Processing delayed deletion for deadline.",
                ?deadline,
                ?now
            );
            let (item, _) = self.queue.pop_front().unwrap();
            self.inner.delete(item).await;
        }
    }

    /// Obtain the next deadline.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.queue.front().map(|(_, instant)| *instant)
    }
}

#[async_trait]
impl<T> super::Write for Writer<T>
where
    T: super::Write + Send,
    <T as super::Write>::Item: Send + Sync,
{
    type Item = <T as super::Write>::Item;

    async fn add(&mut self, item: Self::Item) {
        self.inner.add(item).await
    }

    async fn update(&mut self, item: Self::Item) {
        self.inner.update(item).await
    }

    async fn delete(&mut self, item: Self::Item) {
        let deadline = Instant::now() + self.sleep;
        self.queue.push_back((item, deadline));
    }

    async fn resync(&mut self) {
        self.queue.clear();
        self.inner.resync().await
    }
}

#[async_trait]
impl<T> super::MaintainedWrite for Writer<T>
where
    T: super::MaintainedWrite + Send,
    <T as super::Write>::Item: Send + Sync,
{
    fn maintenance_request(&mut self) -> Option<BoxFuture<'_, ()>> {
        let delayed_delete_deadline = self.next_deadline();
        let downstream = self.inner.maintenance_request();

        match (downstream, delayed_delete_deadline) {
            (Some(downstream), Some(delayed_delete_deadline)) => {
                let fut = timeout_at(delayed_delete_deadline, downstream).map(|_| ());
                Some(Box::pin(fut))
            }
            (None, Some(delayed_delete_deadline)) => {
                Some(Box::pin(sleep_until(delayed_delete_deadline)))
            }
            (Some(downstream), None) => Some(downstream),
            (None, None) => None,
        }
    }

    async fn perform_maintenance(&mut self) {
        // Perform delayed deletes.
        self.perform().await;

        // Do the downstream maintenance.
        self.inner.perform_maintenance().await;
    }
}

#[cfg(test)]
mod tests {
    use futures::{channel::mpsc, SinkExt, StreamExt};
    use k8s_openapi::{api::core::v1::Pod, apimachinery::pkg::apis::meta::v1::ObjectMeta};

    use super::{
        super::{mock, MaintainedWrite, Write},
        *,
    };
    use crate::test_util::trace_init;

    const DELAY_FOR: Duration = Duration::from_secs(3600);

    fn prepare_test() -> (
        Writer<mock::Writer<Pod>>,
        mpsc::Receiver<mock::ScenarioEvent<Pod>>,
        mpsc::Sender<()>,
    ) {
        let (events_tx, events_rx) = mpsc::channel(0);
        let (actions_tx, actions_rx) = mpsc::channel(0);
        let writer = mock::Writer::new(events_tx, actions_rx);
        let writer = Writer::new(writer, DELAY_FOR);
        (writer, events_rx, actions_tx)
    }

    fn make_pod() -> Pod {
        Pod {
            metadata: ObjectMeta {
                name: Some("pod_name".to_owned()),
                uid: Some("pod_uid".to_owned()),
                ..ObjectMeta::default()
            },
            ..Pod::default()
        }
    }

    async fn no_maintenance_test_flow<FT, FA>(ft: FT, fa: FA)
    where
        FT: for<'a> FnOnce(&'a mut (dyn Write<Item = Pod> + Send)) -> BoxFuture<'a, ()>
            + Send
            + 'static,
        FA: FnOnce(mock::ScenarioEvent<Pod>) + Send + 'static,
    {
        tokio::time::pause();
        let (mut writer, mut events_rx, mut actions_tx) = prepare_test();

        // Ensure that right after construction maintenance is not required.
        assert!(writer.maintenance_request().is_none());

        let join = {
            tokio::spawn(async move {
                let event = events_rx.next().await.unwrap();
                fa(event);
                actions_tx.send(()).await.unwrap();
            })
        };

        // Ensure that before the operation maintenance is not required.
        assert!(writer.maintenance_request().is_none());

        {
            let fut = ft(&mut writer);
            // pin_mut!(fut);
            fut.await;
        }

        // Ensure that after the operation maintenance is not required.
        assert!(writer.maintenance_request().is_none());

        join.await.unwrap();
        tokio::time::resume();

        // Ensure that finally maintenance is not required.
        assert!(writer.maintenance_request().is_none());
    }

    #[tokio::test]
    async fn add() {
        trace_init();

        let pod = make_pod();
        let assert_pod = pod.clone();
        no_maintenance_test_flow(
            |writer| Box::pin(writer.add(pod)),
            |event| assert_eq!(event.unwrap_op(), (assert_pod, mock::OpKind::Add)),
        )
        .await
    }

    #[tokio::test]
    async fn update() {
        trace_init();

        let pod = make_pod();
        let assert_pod = pod.clone();
        no_maintenance_test_flow(
            |writer| Box::pin(writer.update(pod)),
            |event| assert_eq!(event.unwrap_op(), (assert_pod, mock::OpKind::Update)),
        )
        .await
    }

    #[tokio::test]
    async fn delete() {
        trace_init();

        // Freeze time.
        tokio::time::pause();

        // Prepare test parameters.
        let (mut writer, mut events_rx, mut actions_tx) = prepare_test();

        // Ensure that right after construction maintenance is not required.
        assert!(writer.maintenance_request().is_none());

        // Prepare a mock pod.
        let pod = make_pod();

        writer.delete(pod.clone()).await;

        // Ensure the deletion event didn't trigger the actual deletion immediately.
        assert!(events_rx.try_next().is_err());

        // Ensure maintenance request is now present.
        let maintenance_request = writer
            .maintenance_request()
            .expect("maintenance request should be present");

        // Advance time.
        tokio::time::advance(DELAY_FOR * 2).await;

        // At this point, maintenance request should be ready.
        maintenance_request.await;

        // Run the assertion on the delete operation to ensure maintenance
        // actually causes a delete.
        let join = tokio::spawn(async move {
            // Control for the deletion action.
            let event = events_rx.next().await.unwrap();
            assert_eq!(event.unwrap_op(), (pod, mock::OpKind::Delete));
            actions_tx.send(()).await.unwrap();

            // Control for the mock perform maintenance call (downstream maintenance).
            let event = events_rx.next().await.unwrap();
            assert!(matches!(event, mock::ScenarioEvent::Maintenance));
            actions_tx.send(()).await.unwrap();
        });

        // Perform maintenance.
        writer.perform_maintenance().await;

        // Join on assertion to guarantee panic propagation.
        join.await.unwrap();

        // Unfreeze time.
        tokio::time::resume();
    }

    #[tokio::test]
    async fn resync() {
        trace_init();

        no_maintenance_test_flow(
            |writer| Box::pin(writer.resync()),
            |event| assert!(matches!(event, mock::ScenarioEvent::Resync)),
        )
        .await
    }
}
