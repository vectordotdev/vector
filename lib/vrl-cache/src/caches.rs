use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use vrl::core::Value;

type CacheMap = HashMap<String, VrlCache>;

#[derive(Default, Debug)]
pub struct VrlCache {
    pub data: HashMap<String, Value>,
}

#[derive(Clone, Default, Debug)]
pub struct VrlCacheRegistry {
    pub caches: Arc<RwLock<CacheMap>>,
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

    pub fn insert_caches(&self, new_caches: CacheMap) {
        let mut caches = self.caches.write().unwrap();
        caches.extend(new_caches);
    }
}

/// Provides read only access to the enrichment tables via the
/// `vrl::EnrichmentTableSearch` trait. Cloning this object is designed to be
/// cheap. The underlying data will be shared by all clones.
#[derive(Clone, Default, Debug)]
pub struct VrlCacheSearch(Arc<RwLock<CacheMap>>);

impl VrlCacheSearch {
    pub fn get_val(&self, cache: &String, key: &String) -> Option<Value> {
        let locked = self.0.read().unwrap();
        locked[cache].data.get(key).cloned()
    }
}
