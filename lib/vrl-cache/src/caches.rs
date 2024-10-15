use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use vector_common::byte_size_of::ByteSizeOf;
use vector_common::internal_event::emit;
use vrl::core::Value;

use crate::internal_events::{VrlCacheDeleted, VrlCacheInserted, VrlCacheRead, VrlCacheReadFailed};

type CacheMap = BTreeMap<String, VrlCache>;

#[derive(Default, Debug)]
pub struct VrlCache {
    data: BTreeMap<String, Value>,
}

#[derive(Clone, Default, Debug)]
pub struct VrlCacheRegistry {
    caches: Arc<RwLock<CacheMap>>,
}

/// Eq implementation for caching purposes
impl PartialEq for VrlCacheRegistry {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.caches, &other.caches)
    }
}
impl Eq for VrlCacheRegistry {}

impl VrlCacheRegistry {
    /// Return a list of the available caches we can read and write to.
    ///
    /// # Panics
    ///
    /// Panics if the RwLock is poisoned.
    pub fn cache_ids(&self) -> Vec<String> {
        let locked = self.caches.read().unwrap();
        locked.iter().map(|(key, _)| key.clone()).collect()
    }

    /// Returns a cheaply clonable struct through that provides lock free read
    /// access to the cache.
    pub fn as_readonly(&self) -> VrlCacheSearch {
        VrlCacheSearch(self.caches.clone())
    }

    pub fn insert_caches(&self, new_caches: impl IntoIterator<Item = (String, VrlCache)>) {
        let mut caches = self.caches.write().unwrap();
        caches.extend(new_caches);
    }

    pub fn writer(&self) -> VrlCacheWriter {
        VrlCacheWriter(self.caches.clone())
    }
}

/// Provides read only access to VRL Caches
#[derive(Clone, Default, Debug)]
pub struct VrlCacheSearch(Arc<RwLock<CacheMap>>);

impl VrlCacheSearch {
    pub fn get_val(&self, cache: &str, key: &str) -> Option<Value> {
        let locked = self.0.read().unwrap();
        let result = locked[cache].data.get(key).cloned();
        match result {
            Some(_) => emit(VrlCacheRead {
                cache: cache.to_string(),
                key: key.to_string(),
            }),
            None => emit(VrlCacheReadFailed {
                cache: cache.to_string(),
                key: key.to_string(),
            }),
        }
        result
    }
}

/// Provides write access to VRL caches
#[derive(Clone, Default, Debug)]
pub struct VrlCacheWriter(Arc<RwLock<CacheMap>>);

impl VrlCacheWriter {
    pub fn put_val(&self, cache: &str, key: &str, value: Value) {
        let mut locked = self.0.write().unwrap();
        locked
            .get_mut(cache)
            .unwrap()
            .data
            .insert(key.to_string(), value);
        emit(VrlCacheInserted {
            cache: cache.to_string(),
            key: key.to_string(),
            new_objects_count: locked[cache].data.keys().len(),
            new_byte_size: locked[cache].data.size_of(),
        });
    }

    pub fn delete_val(&self, cache: &str, key: &str) -> Option<Value> {
        let mut locked = self.0.write().unwrap();
        let result = locked.get_mut(cache).unwrap().data.remove(key);
        emit(VrlCacheDeleted {
            cache: cache.to_string(),
            key: key.to_string(),
            new_objects_count: locked[cache].data.keys().len(),
            new_byte_size: locked[cache].data.size_of(),
        });
        result
    }
}
