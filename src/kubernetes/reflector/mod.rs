//! Caches objects in memory

mod object_ref;
pub mod store;

pub use self::object_ref::{Extra as ObjectRefExtra, ObjectRef};
use futures::{Stream, TryStreamExt};
use kube::api::Resource;
use kube::runtime::watcher;
use std::hash::Hash;
pub use store::Store;

/// Caches objects from `watcher::Event`s to a local `Store`
///
/// Keep in mind that the `Store` is just a cache, and may be out of date.
///
/// Note: It is a bad idea to feed a single `reflector` from multiple `watcher`s, since
/// the whole `Store` will be cleared whenever any of them emits a `Restarted` event.
pub fn reflector<K, W>(mut store: store::Writer<K>, stream: W) -> impl Stream<Item = W::Item>
where
    K: Resource + Clone,
    K::DynamicType: Eq + Hash + Clone,
    W: Stream<Item = watcher::Result<watcher::Event<K>>>,
{
    stream.inspect_ok(move |event| store.apply_watcher_event(event))
}

#[cfg(test)]
mod tests {
    use super::{reflector, store, ObjectRef};
    use futures::{stream, StreamExt, TryStreamExt};
    use k8s_openapi::{api::core::v1::ConfigMap, apimachinery::pkg::apis::meta::v1::ObjectMeta};
    use kube::runtime::watcher;
    use rand::{
        distributions::{Bernoulli, Uniform},
        Rng,
    };
    use std::collections::{BTreeMap, HashMap};

    #[tokio::test]
    async fn reflector_applied_should_add_object() {
        let store_w = store::Writer::default();
        let store = store_w.as_reader();
        let cm = ConfigMap {
            metadata: ObjectMeta {
                name: Some("a".to_string()),
                ..ObjectMeta::default()
            },
            ..ConfigMap::default()
        };
        reflector(
            store_w,
            stream::iter(vec![Ok(watcher::Event::Applied(cm.clone()))]),
        )
        .map(|_| ())
        .collect::<()>()
        .await;
        assert_eq!(store.get(&ObjectRef::from_obj(&cm)).as_deref(), Some(&cm));
    }

    #[tokio::test]
    async fn reflector_applied_should_update_object() {
        let store_w = store::Writer::default();
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
        reflector(
            store_w,
            stream::iter(vec![
                Ok(watcher::Event::Applied(cm.clone())),
                Ok(watcher::Event::Applied(updated_cm.clone())),
            ]),
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
    async fn reflector_deleted_should_remove_object() {
        let store_w = store::Writer::default();
        let store = store_w.as_reader();
        let cm = ConfigMap {
            metadata: ObjectMeta {
                name: Some("a".to_string()),
                ..ObjectMeta::default()
            },
            ..ConfigMap::default()
        };
        reflector(
            store_w,
            stream::iter(vec![
                Ok(watcher::Event::Applied(cm.clone())),
                Ok(watcher::Event::Deleted(cm.clone())),
            ]),
        )
        .map(|_| ())
        .collect::<()>()
        .await;
        assert_eq!(store.get(&ObjectRef::from_obj(&cm)), None);
    }

    #[tokio::test]
    async fn reflector_restarted_should_clear_objects() {
        let store_w = store::Writer::default();
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
        reflector(
            store_w,
            stream::iter(vec![
                Ok(watcher::Event::Applied(cm_a.clone())),
                Ok(watcher::Event::Restarted(vec![cm_b.clone()])),
            ]),
        )
        .map(|_| ())
        .collect::<()>()
        .await;
        assert_eq!(store.get(&ObjectRef::from_obj(&cm_a)), None);
        assert_eq!(
            store.get(&ObjectRef::from_obj(&cm_b)).as_deref(),
            Some(&cm_b)
        );
    }

    #[tokio::test]
    async fn reflector_store_should_not_contain_duplicates() {
        let mut rng = rand::thread_rng();
        let item_dist = Uniform::new(0_u8, 100);
        let deleted_dist = Bernoulli::new(0.40).unwrap();
        let store_w = store::Writer::default();
        let store = store_w.as_reader();
        reflector(
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
