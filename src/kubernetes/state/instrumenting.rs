//! An instrumenting state wrapper.

use crate::internal_events::kubernetes::instrumenting_state as internal_events;
use async_trait::async_trait;

/// A [`super::Write`] implementatiom that wraps another [`super::Write`] and
/// adds instrumentation.
pub struct Writer<T> {
    inner: T,
}

impl<T> Writer<T> {
    /// Take a [`super::Write`] and return it wrapped with [`Self`].
    pub fn new(inner: T) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl<T> super::Write for Writer<T>
where
    T: super::Write + Send,
{
    type Item = <T as super::Write>::Item;

    async fn add(&mut self, item: Self::Item) {
        emit!(internal_events::StateItemAdded);
        self.inner.add(item).await
    }

    async fn update(&mut self, item: Self::Item) {
        emit!(internal_events::StateItemUpdated);
        self.inner.update(item).await
    }

    async fn delete(&mut self, item: Self::Item) {
        emit!(internal_events::StateItemDeleted);
        self.inner.delete(item).await
    }

    async fn resync(&mut self) {
        emit!(internal_events::StateResynced);
        self.inner.resync().await
    }
}

#[cfg(test)]
mod tests {
    use super::super::{mock, Write};
    use super::*;
    use crate::test_util;
    use futures::{channel::mpsc, SinkExt, StreamExt};
    use k8s_openapi::{api::core::v1::Pod, apimachinery::pkg::apis::meta::v1::ObjectMeta};

    fn prepare_test() -> (
        Writer<mock::Writer<Pod>>,
        mpsc::Receiver<mock::ScenarioEvent<Pod>>,
        mpsc::Sender<()>,
    ) {
        let (events_tx, events_rx) = mpsc::channel(0);
        let (actions_tx, actions_rx) = mpsc::channel(0);
        let writer = mock::Writer::new(events_tx, actions_rx);
        let writer = Writer::new(writer);
        (writer, events_rx, actions_tx)
    }

    fn make_pod() -> Pod {
        Pod {
            metadata: Some(ObjectMeta {
                name: Some("pod_name".to_owned()),
                uid: Some("pod_uid".to_owned()),
                ..ObjectMeta::default()
            }),
            ..Pod::default()
        }
    }

    fn get_metric_value(op_kind: &'static str) -> Option<metrics_runtime::Measurement> {
        let controller = crate::metrics::CONTROLLER.get().unwrap_or_else(|| {
            crate::metrics::init().unwrap();
            crate::metrics::CONTROLLER
                .get()
                .expect("failed to init metric container")
        });

        let key = metrics_core::Key::from_name_and_labels(
            "k8s_state_ops",
            vec![metrics_core::Label::new("op_kind", op_kind)],
        );
        controller
            .snapshot()
            .into_measurements()
            .into_iter()
            .find_map(|(candidate_key, measurement)| {
                if candidate_key == key {
                    Some(measurement)
                } else {
                    None
                }
            })
    }

    fn assert_counter_incremented(
        before: Option<metrics_runtime::Measurement>,
        after: Option<metrics_runtime::Measurement>,
    ) {
        let before = before.unwrap_or_else(|| metrics_runtime::Measurement::Counter(0));
        let after = after.expect("after value was None");

        let (before, after) = match (before, after) {
            (
                metrics_runtime::Measurement::Counter(before),
                metrics_runtime::Measurement::Counter(after),
            ) => (before, after),
            _ => panic!("metrics kind mismatch"),
        };

        let difference = after - before;

        assert_eq!(difference, 1);
    }

    #[test]
    fn add() {
        test_util::trace_init();
        test_util::block_on_std(async {
            let (mut writer, mut events_rx, mut actions_tx) = prepare_test();

            let pod = make_pod();

            let join = {
                let pod = pod.clone();
                let before = get_metric_value("item_added");
                tokio::spawn(async move {
                    assert_eq!(
                        events_rx.next().await.unwrap().unwrap_op(),
                        (pod, mock::OpKind::Add)
                    );

                    // By now metrics should've updated.
                    let after = get_metric_value("item_added");
                    assert_counter_incremented(before, after);

                    actions_tx.send(()).await.unwrap();
                })
            };

            writer.add(pod).await;
            join.await.unwrap();
        })
    }

    #[test]
    fn update() {
        test_util::trace_init();
        test_util::block_on_std(async {
            let (mut writer, mut events_rx, mut actions_tx) = prepare_test();

            let pod = make_pod();

            let join = {
                let pod = pod.clone();
                let before = get_metric_value("item_updated");
                tokio::spawn(async move {
                    assert_eq!(
                        events_rx.next().await.unwrap().unwrap_op(),
                        (pod, mock::OpKind::Update)
                    );

                    // By now metrics should've updated.
                    let after = get_metric_value("item_updated");
                    assert_counter_incremented(before, after);

                    actions_tx.send(()).await.unwrap();
                })
            };

            writer.update(pod).await;
            join.await.unwrap();
        })
    }

    #[test]
    fn delete() {
        test_util::trace_init();
        test_util::block_on_std(async {
            let (mut writer, mut events_rx, mut actions_tx) = prepare_test();

            let pod = make_pod();

            let join = {
                let pod = pod.clone();
                let before = get_metric_value("item_deleted");
                tokio::spawn(async move {
                    assert_eq!(
                        events_rx.next().await.unwrap().unwrap_op(),
                        (pod, mock::OpKind::Delete)
                    );

                    // By now metrics should've updated.
                    let after = get_metric_value("item_deleted");
                    assert_counter_incremented(before, after);

                    actions_tx.send(()).await.unwrap();
                })
            };

            writer.delete(pod).await;
            join.await.unwrap();
        })
    }

    #[test]
    fn resync() {
        test_util::trace_init();
        test_util::block_on_std(async {
            let (mut writer, mut events_rx, mut actions_tx) = prepare_test();

            let join = {
                let before = get_metric_value("resynced");
                tokio::spawn(async move {
                    assert!(matches!(
                        events_rx.next().await.unwrap(),
                        mock::ScenarioEvent::Resync
                    ));

                    let after = get_metric_value("resynced");
                    assert_counter_incremented(before, after);

                    actions_tx.send(()).await.unwrap();
                })
            };

            writer.resync().await;
            join.await.unwrap();
        })
    }
}
