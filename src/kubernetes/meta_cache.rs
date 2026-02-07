use std::collections::HashSet;

use kube::core::ObjectMeta;

#[derive(Default)]
pub struct MetaCache {
    pub cache: HashSet<MetaDescribe>,
}

impl MetaCache {
    pub fn new() -> Self {
        Self {
            cache: HashSet::new(),
        }
    }
    pub fn store(&mut self, meta_desc: MetaDescribe) {
        self.cache.insert(meta_desc);
    }
    pub fn delete(&mut self, meta_desc: &MetaDescribe) {
        self.cache.remove(meta_desc);
    }
    pub fn contains(&self, meta_desc: &MetaDescribe) -> bool {
        self.cache.contains(meta_desc)
    }
    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

#[derive(PartialEq, Eq, Hash, Clone)]
pub struct MetaDescribe {
    name: String,
    namespace: String,
    uid: String,
}

impl MetaDescribe {
    pub fn from_meta(meta: &ObjectMeta) -> Self {
        let name = meta.name.clone().unwrap_or_default();
        let namespace = meta.namespace.clone().unwrap_or_default();
        let uid = meta.uid.clone().unwrap_or_default();

        Self {
            name,
            namespace,
            uid,
        }
    }
}

#[cfg(test)]
mod tests {
    use kube::core::ObjectMeta;

    use super::{MetaCache, MetaDescribe};

    #[test]
    fn cache_should_store_data() {
        let mut meta_cache = MetaCache::new();

        let obj_meta = ObjectMeta {
            name: Some("a".to_string()),
            namespace: Some("b".to_string()),
            ..ObjectMeta::default()
        };
        let meta_desc = MetaDescribe::from_meta(&obj_meta);

        meta_cache.store(meta_desc);
        assert_eq!(1, meta_cache.cache.len());
    }

    #[test]
    fn cache_should_delete_data() {
        let mut meta_cache = MetaCache::new();

        let obj_meta = ObjectMeta {
            name: Some("a".to_string()),
            namespace: Some("b".to_string()),
            ..ObjectMeta::default()
        };
        let meta_desc = MetaDescribe::from_meta(&obj_meta);

        meta_cache.store(meta_desc.clone());
        assert_eq!(1, meta_cache.cache.len());
        meta_cache.delete(&meta_desc);
        assert_eq!(0, meta_cache.cache.len());
    }

    #[test]
    fn cache_should_check_active() {
        let mut meta_cache = MetaCache::new();

        let obj_meta = ObjectMeta {
            name: Some("a".to_string()),
            namespace: Some("b".to_string()),
            ..ObjectMeta::default()
        };
        let meta_desc = MetaDescribe::from_meta(&obj_meta);

        meta_cache.store(meta_desc.clone());
        assert!(meta_cache.contains(&meta_desc));
    }
}
