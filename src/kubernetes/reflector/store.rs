use super::ObjectRef;
use ahash::AHashMap;
use derivative::Derivative;
use kube::api::Resource;
use kube::runtime::watcher;
use parking_lot::RwLock;
use std::{fmt::Debug, hash::Hash, sync::Arc};

type Cache<K> = Arc<RwLock<AHashMap<ObjectRef<K>, Arc<K>>>>;

/// A writable Store handle
///
/// This is exclusive since it's not safe to share a single `Store` between multiple reflectors.
/// In particular, `Restarted` events will clobber the state of other connected reflectors.
#[derive(Debug, Derivative)]
#[derivative(Default(bound = "K::DynamicType: Default"))]
pub struct Writer<K: 'static + Resource>
where
    K::DynamicType: Eq + Hash,
{
    store: Cache<K>,
    dyntype: K::DynamicType,
}

impl<K: 'static + Resource + Clone> Writer<K>
where
    K::DynamicType: Eq + Hash + Clone,
{
    /// Creates a new Writer with the specified dynamic type.
    ///
    /// If the dynamic type is default-able (for example when writer is used with
    /// `k8s_openapi` types) you can use `Default` instead.
    pub fn new(dyntype: K::DynamicType) -> Self {
        Writer {
            store: Default::default(),
            dyntype,
        }
    }

    /// Return a read handle to the store
    ///
    /// Multiple read handles may be obtained, by either calling `as_reader` multiple times,
    /// or by calling `Store::clone()` afterwards.
    #[must_use]
    pub fn as_reader(&self) -> Store<K> {
        Store {
            store: self.store.clone(),
        }
    }

    /// Applies a single watcher event to the store
    pub fn apply_watcher_event(&mut self, event: &watcher::Event<K>) {
        match event {
            watcher::Event::Applied(obj) => {
                let key = ObjectRef::from_obj_with(obj, self.dyntype.clone());
                let obj = Arc::new(obj.clone());
                self.store.write().insert(key, obj);
            }
            watcher::Event::Deleted(obj) => {
                let key = ObjectRef::from_obj_with(obj, self.dyntype.clone());
                self.store.write().remove(&key);
            }
            watcher::Event::Restarted(new_objs) => {
                let new_objs = new_objs
                    .iter()
                    .map(|obj| {
                        (
                            ObjectRef::from_obj_with(obj, self.dyntype.clone()),
                            Arc::new(obj.clone()),
                        )
                    })
                    .collect::<AHashMap<_, _>>();
                *self.store.write() = new_objs;
            }
        }
    }
}

/// A readable cache of Kubernetes objects of kind `K`
///
/// Cloning will produce a new reference to the same backing store.
///
/// Cannot be constructed directly since one writer handle is required,
/// use `Writer::as_reader()` instead.
#[derive(Derivative)]
#[derivative(Debug(bound = "K: Debug, K::DynamicType: Debug"), Clone)]
pub struct Store<K: 'static + Resource>
where
    K::DynamicType: Hash + Eq,
{
    store: Cache<K>,
}

impl<K: 'static + Clone + Resource> Store<K>
where
    K::DynamicType: Eq + Hash + Clone,
{
    /// Retrieve a `clone()` of the entry referred to by `key`, if it is in the cache.
    ///
    /// `key.namespace` is ignored for cluster-scoped resources.
    ///
    /// Note that this is a cache and may be stale. Deleted objects may still exist in the cache
    /// despite having been deleted in the cluster, and new objects may not yet exist in the cache.
    /// If any of these are a problem for you then you should abort your reconciler and retry later.
    /// If you use `kube_rt::controller` then you can do this by returning an error and specifying a
    /// reasonable `error_policy`.
    #[must_use]
    pub fn get(&self, key: &ObjectRef<K>) -> Option<Arc<K>> {
        let store = self.store.read();
        store
            .get(key)
            // Try to erase the namespace and try again, in case the object is cluster-scoped
            .or_else(|| {
                store.get(&{
                    let mut cluster_key = key.clone();
                    cluster_key.namespace = None;
                    cluster_key
                })
            })
            // Clone to let go of the entry lock ASAP
            .cloned()
    }

    /// Return a full snapshot of the current values
    #[must_use]
    pub fn state(&self) -> Vec<Arc<K>> {
        let s = self.store.read();
        s.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::Writer;
    use crate::kubernetes::reflector::ObjectRef;
    use k8s_openapi::api::core::v1::ConfigMap;
    use kube::api::ObjectMeta;
    use kube::runtime::watcher;

    #[test]
    fn should_allow_getting_namespaced_object_by_namespaced_ref() {
        let cm = ConfigMap {
            metadata: ObjectMeta {
                name: Some("obj".to_string()),
                namespace: Some("ns".to_string()),
                ..ObjectMeta::default()
            },
            ..ConfigMap::default()
        };
        let mut store_w = Writer::default();
        store_w.apply_watcher_event(&watcher::Event::Applied(cm.clone()));
        let store = store_w.as_reader();
        assert_eq!(store.get(&ObjectRef::from_obj(&cm)).as_deref(), Some(&cm));
    }

    #[test]
    fn should_not_allow_getting_namespaced_object_by_clusterscoped_ref() {
        let cm = ConfigMap {
            metadata: ObjectMeta {
                name: Some("obj".to_string()),
                namespace: Some("ns".to_string()),
                ..ObjectMeta::default()
            },
            ..ConfigMap::default()
        };
        let mut cluster_cm = cm.clone();
        cluster_cm.metadata.namespace = None;
        let mut store_w = Writer::default();
        store_w.apply_watcher_event(&watcher::Event::Applied(cm));
        let store = store_w.as_reader();
        assert_eq!(store.get(&ObjectRef::from_obj(&cluster_cm)), None);
    }

    #[test]
    fn should_allow_getting_clusterscoped_object_by_clusterscoped_ref() {
        let cm = ConfigMap {
            metadata: ObjectMeta {
                name: Some("obj".to_string()),
                namespace: None,
                ..ObjectMeta::default()
            },
            ..ConfigMap::default()
        };
        let mut store_w = Writer::default();
        store_w.apply_watcher_event(&watcher::Event::Applied(cm.clone()));
        let store = store_w.as_reader();
        assert_eq!(store.get(&ObjectRef::from_obj(&cm)).as_deref(), Some(&cm));
    }

    #[test]
    fn should_allow_getting_clusterscoped_object_by_namespaced_ref() {
        let cm = ConfigMap {
            metadata: ObjectMeta {
                name: Some("obj".to_string()),
                namespace: None,
                ..ObjectMeta::default()
            },
            ..ConfigMap::default()
        };
        let mut nsed_cm = cm.clone();
        nsed_cm.metadata.namespace = Some("ns".to_string());
        let mut store_w = Writer::default();
        store_w.apply_watcher_event(&watcher::Event::Applied(cm.clone()));
        let store = store_w.as_reader();
        assert_eq!(
            store.get(&ObjectRef::from_obj(&nsed_cm)).as_deref(),
            Some(&cm)
        );
    }
}
