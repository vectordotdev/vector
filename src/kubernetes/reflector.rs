//! Intercept [`watcher::Event`]'s.

use futures::StreamExt;
use futures_util::Stream;
use kube::{
    runtime::{reflector::store, watcher},
    Resource,
};
use std::{hash::Hash, time::Duration};
use tokio::pin;
use tokio_util::time::DelayQueue;

/// Handles events from a [`kube::runtime::watcher`] to delay the application of Deletion events.
pub async fn custom_reflector<K, W>(
    mut store: store::Writer<K>,
    stream: W,
    delay_deletion: Duration,
) where
    K: Resource + Clone,
    K::DynamicType: Eq + Hash + Clone,
    W: Stream<Item = watcher::Result<watcher::Event<K>>>,
{
    pin!(stream);
    let mut delay_queue = DelayQueue::default();
    loop {
        tokio::select! {
            Some(Ok(event)) = stream.next() => {
                match event {
                    watcher::Event::Applied(_) => {
                        // Immediately apply event
                        store.apply_watcher_event(&event);
                    }
                    watcher::Event::Deleted(_) => {
                        // Delay reconciling any `Deleted` events
                        delay_queue.insert(event.to_owned(), delay_deletion);
                    }
                    watcher::Event::Restarted(_) => {
                        // Clear all delayed events when the cache is refreshed
                        delay_queue.clear();
                        store.apply_watcher_event(&event);
                    }
                }
            }
            // When the Deleted event expires from the queue pass it to the store
            Some(Ok(event)) = delay_queue.next() => {
                store.apply_watcher_event(&event.into_inner())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::custom_reflector;
    use futures::channel::mpsc;
    use futures_util::SinkExt;
    use k8s_openapi::{api::core::v1::ConfigMap, apimachinery::pkg::apis::meta::v1::ObjectMeta};
    use kube::runtime::{
        reflector::{store, ObjectRef},
        watcher,
    };
    use std::time::Duration;

    #[tokio::test]
    async fn applied_should_add_object() {
        let store_w = store::Writer::default();
        let store = store_w.as_reader();
        let cm = ConfigMap {
            metadata: ObjectMeta {
                name: Some("a".to_string()),
                ..ObjectMeta::default()
            },
            ..ConfigMap::default()
        };
        let (mut tx, rx) = mpsc::channel::<_>(5);
        tx.send(Ok(watcher::Event::Applied(cm.clone())))
            .await
            .unwrap();
        tokio::spawn(custom_reflector(store_w, rx, Duration::from_secs(1)));
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(store.get(&ObjectRef::from_obj(&cm)).as_deref(), Some(&cm));
    }

    #[tokio::test]
    async fn deleted_should_remove_object_after_delay() {
        let store_w = store::Writer::default();
        let store = store_w.as_reader();
        let cm = ConfigMap {
            metadata: ObjectMeta {
                name: Some("a".to_string()),
                ..ObjectMeta::default()
            },
            ..ConfigMap::default()
        };
        let (mut tx, rx) = mpsc::channel::<_>(5);
        tx.send(Ok(watcher::Event::Applied(cm.clone())))
            .await
            .unwrap();
        tx.send(Ok(watcher::Event::Deleted(cm.clone())))
            .await
            .unwrap();
        tokio::spawn(custom_reflector(store_w, rx, Duration::from_secs(2)));
        // Ensure the Resource is still available after deletion
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(store.get(&ObjectRef::from_obj(&cm)).as_deref(), Some(&cm));
        // Ensure the Resource is removed once the `delay_deletion` has elapsed
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert_eq!(store.get(&ObjectRef::from_obj(&cm)), None);
    }
}
