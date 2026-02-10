//! Template cache stub for NetFlow v5.
//!
//! NetFlow v5 uses a fixed record format and does not use templates. This module
//! provides a minimal TemplateCache so the source and protocol parser share the
//! same interface. Full template management (for NetFlow v9 and IPFIX) will be
//! added in a follow-up PR.

use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::Instant;

/// Unique identifier for a template.
pub type TemplateKey = (SocketAddr, u32, u16);

/// Template field definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateField {
    pub field_type: u16,
    pub field_length: u16,
    pub enterprise_number: Option<u32>,
    pub is_scope: bool,
}

/// Template definition (stub; full layout used by v9/IPFIX in a later PR).
#[derive(Debug, Clone)]
pub struct Template {
    pub template_id: u16,
    pub fields: Vec<TemplateField>,
    pub scope_field_count: u16,
    pub created: Instant,
    pub last_used: Instant,
    pub usage_count: u64,
}

impl Template {
    pub fn new(template_id: u16, fields: Vec<TemplateField>) -> Self {
        let now = Instant::now();
        Self {
            template_id,
            fields,
            scope_field_count: 0,
            created: now,
            last_used: now,
            usage_count: 0,
        }
    }

    pub fn new_options(template_id: u16, fields: Vec<TemplateField>, scope_field_count: u16) -> Self {
        let now = Instant::now();
        Self {
            template_id,
            fields,
            scope_field_count,
            created: now,
            last_used: now,
            usage_count: 0,
        }
    }
}

/// Buffered data record (stub; used when full template management is added).
#[derive(Debug, Clone)]
pub struct BufferedDataRecord {
    pub data: Vec<u8>,
    pub buffered_at: Instant,
    pub peer_addr: SocketAddr,
    pub observation_domain_id: u32,
}

#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub insertions: u64,
    pub evictions: u64,
    pub expired_removals: u64,
    pub current_size: usize,
}

impl CacheStats {
    pub fn hit_ratio(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

/// Minimal template cache for NetFlow v5. No templates are stored; the cache is a no-op.
pub struct TemplateCache {
    max_size: usize,
    _max_buffered_records: usize,
    stats: Arc<RwLock<CacheStats>>,
}

impl Clone for TemplateCache {
    fn clone(&self) -> Self {
        Self {
            max_size: self.max_size,
            _max_buffered_records: self._max_buffered_records,
            stats: Arc::clone(&self.stats),
        }
    }
}

impl TemplateCache {
    pub fn new(max_size: usize) -> Self {
        Self::new_with_buffering(max_size, 100)
    }

    pub fn new_with_buffering(max_size: usize, max_buffered_records: usize) -> Self {
        Self {
            max_size,
            _max_buffered_records: max_buffered_records,
            stats: Arc::new(RwLock::new(CacheStats::default())),
        }
    }

    pub fn get(&self, _key: &TemplateKey) -> Option<Arc<Template>> {
        None
    }

    pub fn insert(&self, _key: TemplateKey, _template: Template) {}

    pub fn cleanup_expired(&self, _timeout_seconds: u64) {}

    pub fn stats(&self) -> CacheStats {
        self.stats
            .read()
            .map(|s| s.clone())
            .unwrap_or_else(|_| CacheStats::default())
    }

    pub fn len(&self) -> usize {
        0
    }

    pub fn is_empty(&self) -> bool {
        true
    }

    pub fn clear(&self) {}

    pub fn debug_templates(&self, _limit: usize) -> Vec<(TemplateKey, Template)> {
        Vec::new()
    }

    pub fn process_buffered_records(&self, _key: TemplateKey) -> Vec<BufferedDataRecord> {
        Vec::new()
    }

    pub fn buffer_data_record(
        &self,
        _key: TemplateKey,
        _data: Vec<u8>,
        _peer_addr: SocketAddr,
        _observation_domain_id: u32,
    ) -> bool {
        true
    }

    pub fn get_buffered_records(&self, _key: &TemplateKey) -> Vec<BufferedDataRecord> {
        Vec::new()
    }

    pub fn cleanup_expired_buffered_records(&self, _timeout: std::time::Duration) -> usize {
        0
    }

    pub fn buffered_stats(&self) -> (usize, usize) {
        (0, 0)
    }
}

impl std::fmt::Debug for TemplateCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let stats = self.stats();
        f.debug_struct("TemplateCache")
            .field("max_size", &self.max_size)
            .field("current_size", &stats.current_size)
            .field("hit_ratio", &stats.hit_ratio())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn test_key() -> TemplateKey {
        (
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 2055),
            1,
            256,
        )
    }

    #[test]
    fn test_cache_empty() {
        let cache = TemplateCache::new(10);
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert!(cache.get(&test_key()).is_none());
    }

    #[test]
    fn test_cache_insert_get_no_op() {
        let cache = TemplateCache::new_with_buffering(10, 100);
        let template = Template::new(256, vec![]);
        cache.insert(test_key(), template);
        assert!(cache.get(&test_key()).is_none());
    }
}
