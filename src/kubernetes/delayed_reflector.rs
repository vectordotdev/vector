use futures::{
    task::{self, Context},
    Stream, TryStreamExt,
};
use kube::api::Resource;
use kube::runtime::reflector::store;
use kube::runtime::watcher;
use std::{hash::Hash, time::Duration};
use tokio_util::time::DelayQueue;

pub fn reflector_shim<K, W>(
    mut store: store::Writer<K>,
    stream: W,
    mut delayer: Delayer<K>,
) -> impl Stream<Item = W::Item>
where
    K: Resource + Clone,
    K::DynamicType: Eq + Hash + Clone,
    W: Stream<Item = watcher::Result<watcher::Event<K>>>,
{
    stream.inspect_ok(move |event| {
        match event {
            watcher::Event::Applied(_) => {
                // Immediately apply event
                store.apply_watcher_event(event);
            }
            watcher::Event::Deleted(_) => {
                // Delay reconciling any `Deleted` events
                delayer.queue.insert(event.to_owned(), delayer.ttl);
            }
            watcher::Event::Restarted(_) => {
                // Clear all delayed events when the cache is refreshed
                delayer.queue.clear();
                store.apply_watcher_event(event);
            }
        } // Check if any events are ready to delete
        while let std::task::Poll::Ready(Some(Ok(event))) = delayer
            .queue
            .poll_expired(&mut Context::from_waker(&task::noop_waker()))
        {
            // Pass the expired event to the underlying store
            store.apply_watcher_event(&event.into_inner());
        }
    })
}

pub struct Delayer<K>
where
    K: Resource + Clone,
    K::DynamicType: Eq + Hash + Clone,
{
    queue: DelayQueue<watcher::Event<K>>,
    ttl: Duration,
}

impl<K> Delayer<K>
where
    K: Resource + Clone,
    K::DynamicType: Eq + Hash + Clone,
{
    pub fn new(ttl: Duration) -> Self {
        Self {
            queue: DelayQueue::default(),
            ttl,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{reflector_shim, Delayer};
    use futures::{stream, StreamExt, TryStreamExt};
    use k8s_openapi::{api::core::v1::ConfigMap, apimachinery::pkg::apis::meta::v1::ObjectMeta};
    use kube::runtime::reflector::store;
    use kube::runtime::reflector::ObjectRef;
    use kube::runtime::watcher;
    use rand::{
        distributions::{Bernoulli, Uniform},
        Rng,
    };
    use std::collections::{BTreeMap, HashMap};
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn reflector_applied_should_add_object() {
        let store_w = store::Writer::default();
        let delayer = Delayer::new(Duration::from_secs(1));
        let store = store_w.as_reader();
        let cm = ConfigMap {
            metadata: ObjectMeta {
                name: Some("a".to_string()),
                ..ObjectMeta::default()
            },
            ..ConfigMap::default()
        };
        reflector_shim(
            store_w,
            stream::iter(vec![Ok(watcher::Event::Applied(cm.clone()))]),
            delayer,
        )
        .map(|_| ())
        .collect::<()>()
        .await;
        assert_eq!(store.get(&ObjectRef::from_obj(&cm)).as_deref(), Some(&cm));
    }

    #[tokio::test]
    async fn reflector_applied_should_update_object() {
        let store_w = store::Writer::default();
        let delayer = Delayer::new(Duration::from_secs(1));
        let store = store_w.as_reader();
        let cm = ConfigMap {
            metadata: ObjectMeta {
                name: Some("a".to_string()),
                ..ObjectMeta::default()
            },
            ..ConfigMap::default()
        };
        let updated_cm = ConfigMap {
            data: Some({
                let mut data = BTreeMap::new();
                data.insert("data".to_string(), "present!".to_string());
                data
            }),
            ..cm.clone()
        };
        reflector_shim(
            store_w,
            stream::iter(vec![
                Ok(watcher::Event::Applied(cm.clone())),
                Ok(watcher::Event::Applied(updated_cm.clone())),
            ]),
            delayer,
        )
        .map(|_| ())
        .collect::<()>()
        .await;
        assert_eq!(
            store.get(&ObjectRef::from_obj(&cm)).as_deref(),
            Some(&updated_cm)
        );
    }

    #[tokio::test]
    async fn reflector_deleted_should_not_immediately_remove_object() {
        let store_w = store::Writer::default();
        let delayer = Delayer::new(Duration::from_secs(1));
        let store = store_w.as_reader();
        let cm = ConfigMap {
            metadata: ObjectMeta {
                name: Some("a".to_string()),
                ..ObjectMeta::default()
            },
            ..ConfigMap::default()
        };
        reflector_shim(
            store_w,
            stream::iter(vec![
                Ok(watcher::Event::Applied(cm.clone())),
                Ok(watcher::Event::Deleted(cm.clone())),
            ]),
            delayer,
        )
        .map(|_| ())
        .collect::<()>()
        .await;
        // We should keep the Applied event in the cache until two things happen
        // the ttl expires, and we receive a new event of any kind
        assert_eq!(store.get(&ObjectRef::from_obj(&cm)).as_deref(), Some(&cm));

        // After waiting past the ttl the next event should actually delete from
        // the cache
        sleep(Duration::from_secs(2)).await;
        // reflector_shim(
        //     store_w,
        //     stream::iter(vec![Ok(watcher::Event::Deleted(cm.clone()))]),
        //     delayer,
        // )
        // .map(|_| ())
        // .collect::<()>()
        // .await;
        // assert_eq!(store.get(&ObjectRef::from_obj(&cm)), None);
    }

    #[tokio::test]
    async fn reflector_restarted_should_clear_objects() {
        let store_w = store::Writer::default();
        let delayer = Delayer::new(Duration::from_secs(1));
        let store = store_w.as_reader();
        let cm_a = ConfigMap {
            metadata: ObjectMeta {
                name: Some("a".to_string()),
                ..ObjectMeta::default()
            },
            ..ConfigMap::default()
        };
        let cm_b = ConfigMap {
            metadata: ObjectMeta {
                name: Some("b".to_string()),
                ..ObjectMeta::default()
            },
            ..ConfigMap::default()
        };
        reflector_shim(
            store_w,
            stream::iter(vec![
                Ok(watcher::Event::Applied(cm_a.clone())),
                Ok(watcher::Event::Restarted(vec![cm_b.clone()])),
            ]),
            delayer,
        )
        .map(|_| ())
        .collect::<()>()
        .await;
        assert_eq!(store.get(&ObjectRef::from_obj(&cm_a)), None);
        assert_eq!(
            store.get(&ObjectRef::from_obj(&cm_b)).as_deref(),
            Some(&cm_b)
        );
        // assert_eq!(delayer.queue.len(), 0);
    }

    #[tokio::test]
    async fn reflector_store_should_not_contain_duplicates() {
        let mut rng = rand::thread_rng();
        let item_dist = Uniform::new(0_u8, 100);
        let deleted_dist = Bernoulli::new(0.40).unwrap();
        let store_w = store::Writer::default();
        let delayer = Delayer::new(Duration::from_secs(1));
        let store = store_w.as_reader();
        reflector_shim(
            store_w,
            stream::iter((0_u32..100_000).map(|gen| {
                let item = rng.sample(item_dist);
                let deleted = rng.sample(deleted_dist);
                let obj = ConfigMap {
                    metadata: ObjectMeta {
                        name: Some(item.to_string()),
                        resource_version: Some(gen.to_string()),
                        ..ObjectMeta::default()
                    },
                    ..ConfigMap::default()
                };
                Ok(if deleted {
                    watcher::Event::Deleted(obj)
                } else {
                    watcher::Event::Applied(obj)
                })
            })),
            delayer,
        )
        .map_ok(|_| ())
        .try_collect::<()>()
        .await
        .unwrap();

        let mut seen_objects = HashMap::new();
        for obj in store.state() {
            assert_eq!(seen_objects.get(obj.metadata.name.as_ref().unwrap()), None);
            seen_objects.insert(obj.metadata.name.clone().unwrap(), obj);
        }
    }
}
