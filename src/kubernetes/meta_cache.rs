use std::collections::HashMap;

use kube::core::ObjectMeta;

pub struct MetaCache {
    pub cache: HashMap<MetaDescribe, bool>
}

impl MetaCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new()
        }
    }
    pub fn store(&mut self, meta_desc: MetaDescribe, active: bool) {
        self.cache.insert(meta_desc, active);
    }
    pub fn delete(&mut self, meta_desc: MetaDescribe) {
        self.cache.retain(|x, _| *x != meta_desc);
    }
    pub fn is_active(&self, meta_descr: &MetaDescribe) -> bool {
        match self.cache.get(&meta_descr) {
            Some(cache_value) => cache_value.to_owned(),
            None => false,
        }
    }

}

#[derive(PartialEq, Eq, Hash, Clone)]
pub struct MetaDescribe {
    name: String,
    namespace: String,
}

impl MetaDescribe {
    pub fn from_meta(meta: &ObjectMeta) -> Self {
        let meta = meta.clone();
        let name = meta.name.unwrap_or_default();
        let namespace = meta.namespace.unwrap_or_default();
        Self {
            name,
            namespace,
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

        meta_cache.store(meta_desc, true);
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

        meta_cache.store(meta_desc.clone(), true);
        assert_eq!(1, meta_cache.cache.len());
        meta_cache.delete(meta_desc);
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

        meta_cache.store(meta_desc.clone(), true);
        assert_eq!(meta_cache.is_active(&meta_desc), true);
        meta_cache.store(meta_desc.clone(), false);
        assert_eq!(meta_cache.is_active(&meta_desc), false);
    }
}
