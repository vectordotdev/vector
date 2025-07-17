//! Template cache types and helpers for NetFlow/IPFIX.

use std::sync::{Arc, RwLock};
use std::time::Instant;
#[cfg(not(test))]
use lru::LruCache;
#[cfg(test)]
use std::collections::HashMap;

// Template field definition
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateField {
    pub field_type: u16,
    pub field_length: u16,
    pub enterprise_number: Option<u32>,
}

// Template definition
#[derive(Debug, Clone)]
pub struct Template {
    #[allow(dead_code)]
    pub template_id: u16,
    pub fields: Vec<TemplateField>,
    pub created: Instant,
}

pub type TemplateKey = (std::net::SocketAddr, u32, u16); // (exporter, observation_domain, template_id)

// Thread-safe template cache with proper cleanup
#[cfg(not(test))]
pub type TemplateCache = Arc<RwLock<LruCache<TemplateKey, Template>>>;
#[cfg(test)]
pub type TemplateCache = Arc<RwLock<HashMap<TemplateKey, Template>>>;

// Constructor function for TemplateCache
pub fn new_template_cache(_capacity: usize) -> TemplateCache {
    #[cfg(not(test))]
    {
        Arc::new(RwLock::new(LruCache::new(_capacity)))
    }
    #[cfg(test)]
    {
        Arc::new(RwLock::new(HashMap::new()))
    }
}

// Thread-safe helper functions for template cache operations
pub fn cache_put(cache: &TemplateCache, key: TemplateKey, value: Template) {
    if let Ok(mut cache) = cache.write() {
        #[cfg(not(test))]
        cache.put(key, value);
        #[cfg(test)]
        cache.insert(key, value);
    }
}

pub fn cache_get(cache: &TemplateCache, key: &TemplateKey) -> Option<Template> {
    #[cfg(not(test))]
    {
        if let Ok(mut cache_guard) = cache.write() {
            cache_guard.get(key).cloned()
        } else {
            None
        }
    }
    #[cfg(test)]
    {
        if let Ok(cache_guard) = cache.read() {
            cache_guard.get(key).cloned()
        } else {
            None
        }
    }
}

pub fn cache_len(cache: &TemplateCache) -> usize {
    if let Ok(cache) = cache.read() {
        cache.len()
    } else {
        0
    }
}

pub fn cleanup_expired_templates(template_cache: &TemplateCache, timeout_secs: u64) {
    use std::time::{Duration, Instant};
    let timeout = Duration::from_secs(timeout_secs);
    if let Ok(mut cache) = template_cache.write() {
        let now = Instant::now();
        let keys_to_remove: Vec<_> = cache.iter()
            .filter_map(|(k, v)| if now.duration_since(v.created) > timeout { Some(k.clone()) } else { None })
            .collect();
        for k in keys_to_remove {
            #[cfg(not(test))]
            cache.pop(&k); // LRU has pop()
            #[cfg(test)]
            cache.remove(&k); // HashMap has remove()
        }
    }
} 