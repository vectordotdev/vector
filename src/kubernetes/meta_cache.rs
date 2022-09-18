use std::collections::HashMap;

use kube::core::ObjectMeta;

pub struct Cacher {
    pub cache: HashMap<MetaDescribe, bool>
}

impl Cacher {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new()
        }
    }
    pub fn store(&mut self, meta_desc: MetaDescribe, active: bool) {
        self.cache.insert(meta_desc, active);
    }
    pub fn is_active(&self, meta_descr: &MetaDescribe) -> bool {
        match self.cache.get(&meta_descr) {
            Some(cache_value) => cache_value.to_owned(),
            None => false,
        }
    }

}
#[derive(PartialEq, Eq, Hash)]
pub struct MetaDescribe {
    name: String,
    namespace: String,
}

impl MetaDescribe {
    pub fn new<T: Into<String>>(name: T, namespace: T) -> Self {
        Self {
            name: name.into(),
            namespace: namespace.into(),
        }
    }

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
