//! An instrumenting state wrapper.

use crate::internal_events::kubernetes::instrumenting_state as internal_events;
use async_trait::async_trait;
use futures::future::BoxFuture;
use k8s_runtime::state;

/// A [`state::Write`] implementatiom that wraps another [`state::Write`] and
/// adds instrumentation.
pub struct Writer<T> {
    inner: T,
}

impl<T> Writer<T> {
    /// Take a [`state::Write`] and return it wrapped with [`Self`].
    pub fn new(inner: T) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl<T> state::Write for Writer<T>
where
    T: state::Write + Send,
{
    type Item = <T as state::Write>::Item;

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

#[async_trait]
impl<T> state::MaintainedWrite for Writer<T>
where
    T: state::MaintainedWrite + Send,
{
    fn maintenance_request(&mut self) -> Option<BoxFuture<'_, ()>> {
        self.inner.maintenance_request().map(|future| {
            emit!(internal_events::StateMaintenanceRequested);
            future
        })
    }

    async fn perform_maintenance(&mut self) {
        emit!(internal_events::StateMaintenancePerformed);
        self.inner.perform_maintenance().await
    }
}

#[cfg(test)]
mod tests {
    use super::Writer;
    use crate::test_util;
    use futures::{channel::mpsc, SinkExt, StreamExt};
    use k8s_openapi::{api::core::v1::Pod, apimachinery::pkg::apis::meta::v1::ObjectMeta};
    use k8s_runtime::state::{mock, MaintainedWrite, Write};
    use once_cell::sync::OnceCell;
    use std::sync::{Mutex, MutexGuard};

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
            metadata: ObjectMeta {
                name: Some("pod_name".to_owned()),
                uid: Some("pod_uid".to_owned()),
                ..ObjectMeta::default()
            },
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

    fn assert_counter_changed(
        before: Option<metrics_runtime::Measurement>,
        after: Option<metrics_runtime::Measurement>,
        expected_difference: u64,
    ) {
        let before = before.unwrap_or_else(|| metrics_runtime::Measurement::Counter(0));
        let after = after.unwrap_or_else(|| metrics_runtime::Measurement::Counter(0));

        let (before, after) = match (before, after) {
            (
                metrics_runtime::Measurement::Counter(before),
                metrics_runtime::Measurement::Counter(after),
            ) => (before, after),
            _ => panic!("metrics kind mismatch"),
        };

        let difference = after - before;

        assert_eq!(difference, expected_difference);
    }

    /// Guarantees only one test will run at a time.
    /// This is required because we assert on a global state, and we don't
    /// want interference.
    fn tests_lock() -> MutexGuard<'static, ()> {
        static INSTANCE: OnceCell<Mutex<()>> = OnceCell::new();
        INSTANCE.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    // TODO: tests here are ignored because they cause interference with
    // the metrics tests.
    // There is no way to assert individual emits, and asserting metrics
    // directly causes issues:
    // - these tests break the internal tests at the metrics implementation
    //   itself, since we end up initializing the metrics controller twice;
    // - testing metrics introduces unintended coupling between subsystems,
    //   ideally we only need to assert that we emit, but avoid assumptions on
    //   what the results of that emit are.
    // Unignore them and/or properly reimplemenmt once the issues above are
    // resolved.

    #[ignore]
    #[test]
    fn add() {
        let _guard = tests_lock();
        test_util::trace_init();
        test_util::block_on_std(async {
            let (mut writer, mut events_rx, mut actions_tx) = prepare_test();

            let pod = make_pod();

            let join = {
                let pod = pod.clone();
                let before = get_metric_value("item_added");
                tokio::spawn(async move {
                    assert_eq!(
                        events_rx.next().await.unwrap().unwrap_item(),
                        (pod, mock::OpKind::Add)
                    );

                    // By now metrics should've updated.
                    let after = get_metric_value("item_added");
                    assert_counter_changed(before, after, 1);

                    actions_tx.send(()).await.unwrap();
                })
            };

            writer.add(pod).await;
            join.await.unwrap();
        })
    }

    #[ignore]
    #[test]
    fn update() {
        let _guard = tests_lock();
        test_util::trace_init();
        test_util::block_on_std(async {
            let (mut writer, mut events_rx, mut actions_tx) = prepare_test();

            let pod = make_pod();

            let join = {
                let pod = pod.clone();
                let before = get_metric_value("item_updated");
                tokio::spawn(async move {
                    assert_eq!(
                        events_rx.next().await.unwrap().unwrap_item(),
                        (pod, mock::OpKind::Update)
                    );

                    // By now metrics should've updated.
                    let after = get_metric_value("item_updated");
                    assert_counter_changed(before, after, 1);

                    actions_tx.send(()).await.unwrap();
                })
            };

            writer.update(pod).await;
            join.await.unwrap();
        })
    }

    #[ignore]
    #[test]
    fn delete() {
        let _guard = tests_lock();
        test_util::trace_init();
        test_util::block_on_std(async {
            let (mut writer, mut events_rx, mut actions_tx) = prepare_test();

            let pod = make_pod();

            let join = {
                let pod = pod.clone();
                let before = get_metric_value("item_deleted");
                tokio::spawn(async move {
                    assert_eq!(
                        events_rx.next().await.unwrap().unwrap_item(),
                        (pod, mock::OpKind::Delete)
                    );

                    // By now metrics should've updated.
                    let after = get_metric_value("item_deleted");
                    assert_counter_changed(before, after, 1);

                    actions_tx.send(()).await.unwrap();
                })
            };

            writer.delete(pod).await;
            join.await.unwrap();
        })
    }

    #[ignore]
    #[test]
    fn resync() {
        let _guard = tests_lock();
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
                    assert_counter_changed(before, after, 1);

                    actions_tx.send(()).await.unwrap();
                })
            };

            writer.resync().await;
            join.await.unwrap();
        })
    }

    #[ignore]
    #[test]
    fn request_maintenance_without_maintenance() {
        let _guard = tests_lock();
        test_util::trace_init();
        test_util::block_on_std(async {
            let (mut writer, _events_rx, _actions_tx) = prepare_test();
            let before = get_metric_value("maintenace_requested");
            let _ = writer.maintenance_request();
            let after = get_metric_value("maintenace_requested");
            assert_counter_changed(before, after, 0);
        })
    }

    #[ignore]
    #[test]
    fn request_maintenance_with_maintenance() {
        let _guard = tests_lock();
        test_util::trace_init();
        test_util::block_on_std(async {
            let (events_tx, _events_rx) = mpsc::channel(0);
            let (_actions_tx, actions_rx) = mpsc::channel(0);
            let (maintenance_request_events_tx, _maintenance_request_events_rx) = mpsc::channel(0);
            let (_maintenance_request_actions_tx, maintenance_request_actions_rx) =
                mpsc::channel(0);
            let writer = mock::Writer::<Pod>::new_with_maintenance(
                events_tx,
                actions_rx,
                maintenance_request_events_tx,
                maintenance_request_actions_rx,
            );
            let mut writer = Writer::new(writer);
            let before = get_metric_value("maintenace_requested");
            let _ = writer.maintenance_request();
            let after = get_metric_value("maintenace_requested");
            assert_counter_changed(before, after, 1);
        })
    }

    #[ignore]
    #[test]
    fn perform_maintenance() {
        let _guard = tests_lock();
        test_util::trace_init();
        test_util::block_on_std(async {
            let (mut writer, mut events_rx, mut actions_tx) = prepare_test();

            let join = {
                let before = get_metric_value("maintenace_performed");
                tokio::spawn(async move {
                    assert!(matches!(
                        events_rx.next().await.unwrap(),
                        mock::ScenarioEvent::Maintenance
                    ));

                    let after = get_metric_value("maintenace_performed");
                    assert_counter_changed(before, after, 1);

                    actions_tx.send(()).await.unwrap();
                })
            };

            writer.perform_maintenance().await;
            join.await.unwrap();
        })
    }
}
