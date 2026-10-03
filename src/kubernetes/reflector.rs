//! Intercept [`watcher::Event`]'s.

use std::{hash::Hash, sync::Arc, time::Duration};

use futures::StreamExt;
use futures_util::Stream;
use kube::{
    Resource,
    runtime::{
        reflector::{ObjectRef, store},
        watcher,
    },
};
use tokio::pin;
use tokio_util::time::DelayQueue;

use super::meta_cache::{MetaCache, MetaDescribe};

/// Handles events from a [`kube::runtime::watcher()`] to delay the application of Deletion events.
pub async fn custom_reflector<K, W>(
    mut store: store::Writer<K>,
    mut meta_cache: MetaCache,
    stream: W,
    delay_deletion: Duration,
) where
    K: Resource + Clone + std::fmt::Debug,
    K::DynamicType: Eq + Hash + Clone + Default,
    W: Stream<Item = watcher::Result<watcher::Event<K>>>,
{
    pin!(stream);
    let mut delay_queue = DelayQueue::default();
    let mut init_buffer_meta = Vec::new();
    loop {
        tokio::select! {
            result = stream.next() => {
                match result {
                    Some(Ok(event)) => {
                        match event {
                            // Immediately reconcile `Apply` event
                            watcher::Event::Apply(ref obj) => {
                                trace!(message = "Processing Apply event.", event_type = std::any::type_name::<K>(), event = ?event);
                                store.apply_watcher_event(&event);
                                let meta_descr = MetaDescribe::from_meta(obj.meta());
                                meta_cache.store(meta_descr);
                            }
                            // Delay reconciling any `Delete` events
                            watcher::Event::Delete(ref obj) => {
                                trace!(message = "Delaying processing Delete event.", event_type = std::any::type_name::<K>(), event = ?event);
                                delay_queue.insert(event.to_owned(), delay_deletion);
                                let meta_descr = MetaDescribe::from_meta(obj.meta());
                                meta_cache.delete(&meta_descr);
                            }
                            // Clear all delayed events on `Init` event
                            watcher::Event::Init => {
                                trace!(message = "Processing Init event.", event_type = std::any::type_name::<K>(), event = ?event);
                                delay_queue.clear();
                                store.apply_watcher_event(&event);
                                meta_cache.clear();
                                init_buffer_meta.clear();
                            }
                            // Immediately reconcile `InitApply` event (but buffer the obj ref so we can handle implied deletions on InitDone)
                            watcher::Event::InitApply(ref obj) => {
                                trace!(message = "Processing InitApply event.", event_type = std::any::type_name::<K>(), event = ?event);
                                store.apply_watcher_event(&event);
                                let meta_descr = MetaDescribe::from_meta(obj.meta());
                                meta_cache.store(meta_descr.clone());
                                init_buffer_meta.push(meta_descr.clone());
                            }
                            // Reconcile `InitApply` events and implied deletions
                            watcher::Event::InitDone => {
                                trace!(message = "Processing InitDone event.", event_type = std::any::type_name::<K>(), event = ?event);
                                store.apply_watcher_event(&event);


                                store.as_reader().state().into_iter()
                                // delay deleting objs that were added before but not during InitApply
                                .for_each(|obj| {
                                    if let Some(inner) = Arc::into_inner(obj) {
                                        let meta_descr = MetaDescribe::from_meta(inner.meta());
                                        if !init_buffer_meta.contains(&meta_descr) {
                                            let implied_deletion_event = watcher::Event::Delete(inner);
                                            trace!(message = "Delaying processing implied deletion.", event_type = std::any::type_name::<K>(), event = ?implied_deletion_event);
                                            delay_queue.insert(implied_deletion_event, delay_deletion);
                                            meta_cache.delete(&meta_descr);
                                        }
                                    }
                                });

                                init_buffer_meta.clear();
                            }
                        }
                    },
                    Some(Err(error)) => {
                        warn!(message = "Watcher Stream received an error. Retrying.", ?error);
                    },
                    // The watcher stream should never yield `None`
                    // https://docs.rs/kube-runtime/0.71.0/src/kube_runtime/watcher.rs.html#234-237
                    None => {
                        unreachable!("a watcher Stream never ends");
                    },
                }
            }
            result = delay_queue.next(), if !delay_queue.is_empty() => {
                match result {
                    Some(event) => {
                        let event = event.into_inner();
                        match event {
                            watcher::Event::Delete(ref obj) => {
                                let meta_desc = MetaDescribe::from_meta(obj.meta());
                                if !meta_cache.contains(&meta_desc)
                                    && store_holds_same_uid(&store, obj)
                                {
                                    trace!(message = "Processing Delete event.", event_type = std::any::type_name::<K>(), event = ?event);
                                    store.apply_watcher_event(&event);
                                } else {
                                    trace!(message = "Skipping delayed Delete event; resource is still in use or was recreated with a new UID.", event_type = std::any::type_name::<K>(), event = ?event);
                                }
                            },
                            _ => store.apply_watcher_event(&event),
                        }
                    },
                    // DelayQueue returns None if the queue is exhausted,
                    // however we disable the DelayQueue branch if there are
                    // no items in the queue.
                    None => {
                        unreachable!("an empty DelayQueue is never polled");
                    },
                }
            }
        }
    }
}

/// Returns `true` if the store either no longer tracks the object's key or
/// still holds the exact same incarnation (matched by UID).
///
/// The reflector store is keyed by name and namespace only. When a resource is
/// recreated with the same name and namespace but a new UID, a delayed `Delete`
/// for the old incarnation must not be reconciled, otherwise it would evict the
/// live resource from the store and stop its logs from being collected. See
/// https://github.com/vectordotdev/vector/issues/12014.
fn store_holds_same_uid<K>(store: &store::Writer<K>, obj: &K) -> bool
where
    K: Resource + Clone + std::fmt::Debug,
    K::DynamicType: Eq + Hash + Clone + Default,
{
    store
        .as_reader()
        .get(&ObjectRef::from_obj(obj))
        .is_none_or(|current| current.meta().uid == obj.meta().uid)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures::channel::mpsc;
    use futures_util::SinkExt;
    use k8s_openapi::{api::core::v1::ConfigMap, apimachinery::pkg::apis::meta::v1::ObjectMeta};
    use kube::runtime::{
        reflector::{ObjectRef, store},
        watcher,
    };

    use super::{MetaCache, custom_reflector};

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
        tx.send(Ok(watcher::Event::Apply(cm.clone())))
            .await
            .unwrap();
        let meta_cache = MetaCache::new();
        tokio::spawn(custom_reflector(
            store_w,
            meta_cache,
            rx,
            Duration::from_secs(1),
        ));
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
        tx.send(Ok(watcher::Event::Apply(cm.clone())))
            .await
            .unwrap();
        tx.send(Ok(watcher::Event::Delete(cm.clone())))
            .await
            .unwrap();
        let meta_cache = MetaCache::new();
        tokio::spawn(custom_reflector(
            store_w,
            meta_cache,
            rx,
            Duration::from_secs(2),
        ));
        // Ensure the Resource is still available after deletion
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(store.get(&ObjectRef::from_obj(&cm)).as_deref(), Some(&cm));
        // Ensure the Resource is removed once the `delay_deletion` has elapsed
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert_eq!(store.get(&ObjectRef::from_obj(&cm)), None);
    }

    #[tokio::test]
    async fn deleted_should_not_remove_object_still_in_use() {
        let store_w = store::Writer::default();
        let store = store_w.as_reader();
        let cm = ConfigMap {
            metadata: ObjectMeta {
                name: Some("name".to_string()),
                namespace: Some("namespace".to_string()),
                ..ObjectMeta::default()
            },
            ..ConfigMap::default()
        };
        let (mut tx, rx) = mpsc::channel::<_>(5);
        tx.send(Ok(watcher::Event::Apply(cm.clone())))
            .await
            .unwrap();
        tx.send(Ok(watcher::Event::Delete(cm.clone())))
            .await
            .unwrap();
        tx.send(Ok(watcher::Event::Apply(cm.clone())))
            .await
            .unwrap();
        let meta_cache = MetaCache::new();
        tokio::spawn(custom_reflector(
            store_w,
            meta_cache,
            rx,
            Duration::from_secs(2),
        ));
        tokio::time::sleep(Duration::from_secs(1)).await;
        // Ensure the Resource is still available after deletion
        assert_eq!(store.get(&ObjectRef::from_obj(&cm)).as_deref(), Some(&cm));
        tokio::time::sleep(Duration::from_secs(5)).await;
        // Ensure the Resource is still available after Applied event
        assert_eq!(store.get(&ObjectRef::from_obj(&cm)).as_deref(), Some(&cm));
    }

    #[tokio::test]
    async fn delayed_delete_should_not_evict_recreated_object_with_new_uid() {
        // Regression test for https://github.com/vectordotdev/vector/issues/12014.
        // A resource recreated with the same name and namespace but a new UID
        // must not be evicted by the delayed `Delete` of its predecessor.
        let store_w = store::Writer::default();
        let store = store_w.as_reader();
        let old = ConfigMap {
            metadata: ObjectMeta {
                name: Some("name".to_string()),
                namespace: Some("namespace".to_string()),
                uid: Some("old-uid".to_string()),
                ..ObjectMeta::default()
            },
            ..ConfigMap::default()
        };
        let new = ConfigMap {
            metadata: ObjectMeta {
                name: Some("name".to_string()),
                namespace: Some("namespace".to_string()),
                uid: Some("new-uid".to_string()),
                ..ObjectMeta::default()
            },
            ..ConfigMap::default()
        };
        let (mut tx, rx) = mpsc::channel::<_>(5);
        // The old incarnation is observed, then replaced by the new incarnation,
        // and only afterwards does the (delayed) deletion of the old one arrive.
        tx.send(Ok(watcher::Event::Apply(old.clone())))
            .await
            .unwrap();
        tx.send(Ok(watcher::Event::Apply(new.clone())))
            .await
            .unwrap();
        tx.send(Ok(watcher::Event::Delete(old.clone())))
            .await
            .unwrap();
        let meta_cache = MetaCache::new();
        tokio::spawn(custom_reflector(
            store_w,
            meta_cache,
            rx,
            Duration::from_secs(2),
        ));
        tokio::time::sleep(Duration::from_secs(1)).await;
        // The live (new) incarnation is present.
        assert_eq!(store.get(&ObjectRef::from_obj(&new)).as_deref(), Some(&new));
        // After the deletion delay elapses, the live incarnation must remain.
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert_eq!(store.get(&ObjectRef::from_obj(&new)).as_deref(), Some(&new));
    }

    #[tokio::test]
    async fn recreate_within_delay_window_should_keep_new_incarnation() {
        // The classic ordering observed in
        // https://github.com/vectordotdev/vector/issues/13467: a pod is deleted
        // and recreated with the same name/namespace but a new UID within the
        // deletion delay window. Adding the UID to the `MetaCache` alone (see
        // https://github.com/vectordotdev/vector/pull/21303) regresses this case
        // because the store is keyed by name/namespace; the delayed deletion of
        // the old UID must not evict the recreated pod.
        let store_w = store::Writer::default();
        let store = store_w.as_reader();
        let old = ConfigMap {
            metadata: ObjectMeta {
                name: Some("name".to_string()),
                namespace: Some("namespace".to_string()),
                uid: Some("old-uid".to_string()),
                ..ObjectMeta::default()
            },
            ..ConfigMap::default()
        };
        let new = ConfigMap {
            metadata: ObjectMeta {
                name: Some("name".to_string()),
                namespace: Some("namespace".to_string()),
                uid: Some("new-uid".to_string()),
                ..ObjectMeta::default()
            },
            ..ConfigMap::default()
        };
        let (mut tx, rx) = mpsc::channel::<_>(5);
        tx.send(Ok(watcher::Event::Apply(old.clone())))
            .await
            .unwrap();
        tx.send(Ok(watcher::Event::Delete(old.clone())))
            .await
            .unwrap();
        tx.send(Ok(watcher::Event::Apply(new.clone())))
            .await
            .unwrap();
        let meta_cache = MetaCache::new();
        tokio::spawn(custom_reflector(
            store_w,
            meta_cache,
            rx,
            Duration::from_secs(2),
        ));
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(store.get(&ObjectRef::from_obj(&new)).as_deref(), Some(&new));
        // The old incarnation's delayed deletion fires here; the new one stays.
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert_eq!(store.get(&ObjectRef::from_obj(&new)).as_deref(), Some(&new));
    }
}
