//! Template management for NetFlow v9 and IPFIX.
//!
//! NetFlow v9 and IPFIX use templates to define the structure of data records.
//! This module provides thread-safe template caching with automatic cleanup.


use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use std::collections::VecDeque;

#[cfg(not(test))]
use lru::LruCache;
#[cfg(not(test))]
use std::num::NonZeroUsize;
#[cfg(test)]
use std::collections::HashMap;

use crate::sources::netflow::events::*;
use tracing::debug;


/// Unique identifier for a template.
/// 
/// Templates are identified by the combination of:
/// - Source address (which exporter sent it)
/// - Observation domain ID (IPFIX) / Source ID (NetFlow v9)
/// - Template ID
pub type TemplateKey = (SocketAddr, u32, u16);

/// Template field definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateField {
    /// IPFIX/NetFlow field type identifier
    pub field_type: u16,
    /// Length of the field in bytes
    pub field_length: u16,
    /// Enterprise number for vendor-specific fields (IPFIX only)
    pub enterprise_number: Option<u32>,
}

/// Template definition containing field layout.
#[derive(Debug, Clone)]
pub struct Template {
    /// Template identifier
    pub template_id: u16,
    /// List of fields in this template
    pub fields: Vec<TemplateField>,
    /// When this template was created/last used
    pub created: Instant,
    /// Last time this template was accessed
    pub last_used: Instant,
    /// Number of times this template has been used
    pub usage_count: u64,
}

/// Buffered data record waiting for template.
#[derive(Debug, Clone)]
pub struct BufferedDataRecord {
    /// Raw data bytes
    pub data: Vec<u8>,
    /// When this record was buffered
    pub buffered_at: Instant,
    /// Peer address that sent this data
    pub peer_addr: SocketAddr,
    /// Observation domain ID
    pub observation_domain_id: u32,
}


impl Template {
    /// Create a new template.
    pub fn new(template_id: u16, fields: Vec<TemplateField>) -> Self {
        let now = Instant::now();
        Self {
            template_id,
            fields,
            created: now,
            last_used: now,
            usage_count: 0,
        }
    }

    /// Mark template as used and update statistics.
    pub fn mark_used(&mut self) {
        self.last_used = Instant::now();
        self.usage_count = self.usage_count.saturating_add(1);
    }

    /// Check if template has expired based on last usage.
    pub fn is_expired(&self, timeout: Duration) -> bool {
        self.last_used.elapsed() > timeout
    }

    /// Calculate total record size in bytes (for fixed-length templates).
    pub fn record_size(&self) -> Option<usize> {
        let mut total_size = 0;
        
        for field in &self.fields {
            // Variable-length fields (length 65535) make the total size indeterminate
            if field.field_length == 65535 {
                return None;
            }
            total_size += field.field_length as usize;
        }
        
        Some(total_size)
    }

    /// Check if template has any variable-length fields.
    pub fn has_variable_fields(&self) -> bool {
        self.fields.iter().any(|f| f.field_length == 65535)
    }
}

/// Thread-safe template cache with LRU eviction and TTL cleanup.
#[derive(Clone)]
pub struct TemplateCache {
    #[cfg(not(test))]
    cache: Arc<RwLock<LruCache<TemplateKey, Template>>>,
    #[cfg(test)]
    cache: Arc<RwLock<HashMap<TemplateKey, Template>>>,
    max_size: usize,
    stats: Arc<RwLock<CacheStats>>,
    /// Buffered data records waiting for templates
    buffered_records: Arc<RwLock<std::collections::HashMap<TemplateKey, VecDeque<BufferedDataRecord>>>>,
    /// Maximum number of records to buffer per template
    max_buffered_records: usize,
}

/// Cache statistics for monitoring and debugging.
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
    /// Calculate cache hit ratio.
    pub fn hit_ratio(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

impl TemplateCache {
    /// Create a new template cache with the specified maximum size.
    pub fn new(max_size: usize) -> Self {
        Self::new_with_buffering(max_size, 100)
    }

    /// Create a new template cache with buffering support.
    pub fn new_with_buffering(max_size: usize, max_buffered_records: usize) -> Self {
        #[cfg(not(test))]
        let cache = Arc::new(RwLock::new(
            LruCache::new(NonZeroUsize::new(max_size).unwrap_or(NonZeroUsize::new(1000).unwrap()))
        ));
        
        #[cfg(test)]
        let cache = Arc::new(RwLock::new(HashMap::new()));

        Self {
            cache,
            max_size,
            stats: Arc::new(RwLock::new(CacheStats::default())),
            buffered_records: Arc::new(RwLock::new(std::collections::HashMap::new())),
            max_buffered_records,
        }
    }

    /// Get a template from the cache.
    pub fn get(&self, key: &TemplateKey) -> Option<Template> {
        #[cfg(not(test))]
        {
            if let Ok(mut cache) = self.cache.write() {
                if let Some(template) = cache.get_mut(key) {
                    template.mark_used();
                    let template_clone = template.clone();
                    let cache_size = cache.len();
                    self.update_stats(|stats| {
                        stats.hits += 1;
                        stats.current_size = cache_size;
                    });
                    return Some(template_clone);
                }
            }
        }

        #[cfg(test)]
        {
            if let Ok(mut cache) = self.cache.write() {
                if let Some(template) = cache.get_mut(key) {
                    template.mark_used();
                    let template_clone = template.clone();
                    let cache_size = cache.len();
                    self.update_stats(|stats| {
                        stats.hits += 1;
                        stats.current_size = cache_size;
                    });
                    return Some(template_clone);
                }
            }
        }

        self.update_stats(|stats| stats.misses += 1);
        None
    }

    /// Insert a template into the cache.
    pub fn insert(&self, key: TemplateKey, template: Template) {
        let mut evicted = false;

        #[cfg(not(test))]
        {
            if let Ok(mut cache) = self.cache.write() {
                // Check if this would cause an eviction
                evicted = cache.len() >= self.max_size && !cache.contains(&key);
                
                cache.put(key, template);
                
                self.update_stats(|stats| {
                    stats.insertions += 1;
                    if evicted {
                        stats.evictions += 1;
                    }
                    stats.current_size = cache.len();
                });
            }
        }

        #[cfg(test)]
        {
            if let Ok(mut cache) = self.cache.write() {
                // For tests, allow unlimited size but track evictions conceptually
                evicted = cache.len() >= self.max_size && !cache.contains_key(&key);
                
                cache.insert(key, template);
                
                self.update_stats(|stats| {
                    stats.insertions += 1;
                    if evicted {
                        stats.evictions += 1;
                    }
                    stats.current_size = cache.len();
                });
            }
        }

        debug!(
            template_id = key.2,
            peer_addr = %key.0,
            observation_domain = key.1,
            evicted = evicted,
            "Template cached"
        );

        // Process any buffered records for this template
        self.process_buffered_records(key);
    }

    /// Process buffered records for a newly available template.
    pub fn process_buffered_records(&self, key: TemplateKey) -> Vec<BufferedDataRecord> {
        self.get_buffered_records(&key)
    }

    /// Remove expired templates from the cache.
    pub fn cleanup_expired(&self, timeout_seconds: u64) {
        let timeout = Duration::from_secs(timeout_seconds);
        let mut removed_count = 0;

        #[cfg(not(test))]
        {
            if let Ok(mut cache) = self.cache.write() {
                let mut keys_to_remove = Vec::new();
                
                // Collect expired keys
                for (key, template) in cache.iter() {
                    if template.is_expired(timeout) {
                        keys_to_remove.push(*key);
                    }
                }
                
                // Remove expired templates
                for key in keys_to_remove {
                    cache.pop(&key);
                    removed_count += 1;
                }
                
                self.update_stats(|stats| {
                    stats.expired_removals += removed_count;
                    stats.current_size = cache.len();
                });
            }
        }

        #[cfg(test)]
        {
            if let Ok(mut cache) = self.cache.write() {
                let keys_to_remove: Vec<_> = cache
                    .iter()
                    .filter(|(_, template)| template.is_expired(timeout))
                    .map(|(key, _)| *key)
                    .collect();
                
                for key in keys_to_remove {
                    cache.remove(&key);
                    removed_count += 1;
                }
                
                self.update_stats(|stats| {
                    stats.expired_removals += removed_count;
                    stats.current_size = cache.len();
                });
            }
        }

        // Also clean up expired buffered records
        let buffered_removed = self.cleanup_expired_buffered_records(timeout);

        if removed_count > 0 || buffered_removed > 0 {
            debug!(
                removed_count = removed_count,
                buffered_removed = buffered_removed,
                timeout_seconds = timeout_seconds,
                "Cleaned up expired templates and buffered records"
            );

            emit!(TemplateCleanupCompleted {
                removed_count: (removed_count as usize + buffered_removed),
                timeout_seconds,
            });
        }
    }

    /// Get current cache statistics.
    pub fn stats(&self) -> CacheStats {
        self.stats.read().map(|s| s.clone()).unwrap_or_else(|_| CacheStats::default())
    }

    /// Get current cache size.
    pub fn len(&self) -> usize {
        #[cfg(not(test))]
        {
            self.cache.read().map(|cache| cache.len()).unwrap_or(0)
        }
        
        #[cfg(test)]
        {
            self.cache.read().map(|cache| cache.len()).unwrap_or(0)
        }
    }

    /// Check if cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear all templates from the cache.
    pub fn clear(&self) {
        #[cfg(not(test))]
        {
            if let Ok(mut cache) = self.cache.write() {
                cache.clear();
                self.update_stats(|stats| stats.current_size = 0);
            }
        }

        #[cfg(test)]
        {
            if let Ok(mut cache) = self.cache.write() {
                cache.clear();
                self.update_stats(|stats| stats.current_size = 0);
            }
        }
    }

    /// Update cache statistics with the given function.
    fn update_stats<F>(&self, update_fn: F)
    where
        F: FnOnce(&mut CacheStats),
    {
        if let Ok(mut stats) = self.stats.write() {
            update_fn(&mut *stats);
        }
    }

    /// Get templates for debugging (returns up to limit templates).
    pub fn debug_templates(&self, limit: usize) -> Vec<(TemplateKey, Template)> {
        #[cfg(not(test))]
        {
            if let Ok(cache) = self.cache.read() {
                cache.iter()
                    .take(limit)
                    .map(|(k, v)| (*k, v.clone()))
                    .collect()
            } else {
                Vec::new()
            }
        }

        #[cfg(test)]
        {
            if let Ok(cache) = self.cache.read() {
                cache.iter()
                    .take(limit)
                    .map(|(k, v)| (*k, v.clone()))
                    .collect()
            } else {
                Vec::new()
            }
        }
    }

    /// Buffer a data record while waiting for its template.
    pub fn buffer_data_record(
        &self,
        key: TemplateKey,
        data: Vec<u8>,
        peer_addr: SocketAddr,
        observation_domain_id: u32,
    ) -> bool {
        if let Ok(mut buffered) = self.buffered_records.write() {
            let queue = buffered.entry(key).or_insert_with(VecDeque::new);
            
            // Check if we've hit the limit
            if queue.len() >= self.max_buffered_records {
                // Remove oldest record
                queue.pop_front();
            }
            
            queue.push_back(BufferedDataRecord {
                data,
                buffered_at: Instant::now(),
                peer_addr,
                observation_domain_id,
            });
            
            true
        } else {
            false
        }
    }

    /// Get and clear buffered records for a template.
    pub fn get_buffered_records(&self, key: &TemplateKey) -> Vec<BufferedDataRecord> {
        if let Ok(mut buffered) = self.buffered_records.write() {
            buffered.remove(key).unwrap_or_default().into()
        } else {
            Vec::new()
        }
    }

    /// Clean up expired buffered records.
    pub fn cleanup_expired_buffered_records(&self, timeout: Duration) -> usize {
        let mut removed = 0;
        
        if let Ok(mut buffered) = self.buffered_records.write() {
            let now = Instant::now();
            let mut keys_to_remove = Vec::new();
            
            for (key, queue) in buffered.iter_mut() {
                while let Some(record) = queue.front() {
                    if now.duration_since(record.buffered_at) > timeout {
                        queue.pop_front();
                        removed += 1;
                    } else {
                        break;
                    }
                }
                
                if queue.is_empty() {
                    keys_to_remove.push(*key);
                }
            }
            
            for key in keys_to_remove {
                buffered.remove(&key);
            }
        }
        
        removed
    }

    /// Get statistics about buffered records.
    pub fn buffered_stats(&self) -> (usize, usize) {
        if let Ok(buffered) = self.buffered_records.read() {
            let total_records: usize = buffered.values().map(|q| q.len()).sum();
            let unique_templates = buffered.len();
            (total_records, unique_templates)
        } else {
            (0, 0)
        }
    }
}

impl std::fmt::Debug for TemplateCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let stats = self.stats();
        f.debug_struct("TemplateCache")
            .field("max_size", &self.max_size)
            .field("current_size", &stats.current_size)
            .field("hit_ratio", &stats.hit_ratio())
            .field("stats", &stats)
            .finish()
    }
}

/// Parse template fields from NetFlow v9 template data.
pub fn parse_netflow_v9_template_fields(data: &[u8]) -> Result<Vec<TemplateField>, String> {
    if data.len() < 4 {
        return Err("Template data too short".to_string());
    }

    let field_count = u16::from_be_bytes([data[2], data[3]]);
    let mut fields = Vec::with_capacity(field_count as usize);
    let mut offset = 4;

    for _ in 0..field_count {
        if offset + 4 > data.len() {
            return Err("Insufficient data for template field".to_string());
        }

        let field_type = u16::from_be_bytes([data[offset], data[offset + 1]]);
        let field_length = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);

        fields.push(TemplateField {
            field_type,
            field_length,
            enterprise_number: None, // NetFlow v9 doesn't use enterprise numbers
        });

        offset += 4;
    }

    Ok(fields)
}

/// Parse template fields from IPFIX template data.
pub fn parse_ipfix_template_fields(data: &[u8]) -> Result<Vec<TemplateField>, String> {
    if data.len() < 4 {
        return Err("Template data too short".to_string());
    }

    let field_count = u16::from_be_bytes([data[2], data[3]]);
    let mut fields = Vec::with_capacity(field_count as usize);
    let mut offset = 4;

    for _ in 0..field_count {
        if offset + 4 > data.len() {
            return Err("Insufficient data for template field".to_string());
        }

        let raw_field_type = u16::from_be_bytes([data[offset], data[offset + 1]]);
        let field_length = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);
        offset += 4;

        let (field_type, enterprise_number) = if raw_field_type & 0x8000 != 0 {
            // Enterprise field - next 4 bytes contain enterprise number
            if offset + 4 > data.len() {
                // Debug: Log raw template data for template ID 1024
                if data.len() >= 2 {
                    let template_id = u16::from_be_bytes([data[0], data[1]]);
                    if template_id == 1024 {
                        error!(
                            message = "Template ID 1024 enterprise field parsing failed - raw data dump",
                            template_id = template_id,
                            data_length = data.len(),
                            offset = offset,
                            remaining_bytes = data.len() - offset,
                            raw_data_hex = format!("{:02x?}", data),
                            raw_data_base64 = base64::engine::general_purpose::STANDARD.encode(data),
                        );
                    }
                }
                return Err("Insufficient data for enterprise field".to_string());
            }

            let enterprise_id = u32::from_be_bytes([
                data[offset],
                data[offset + 1], 
                data[offset + 2],
                data[offset + 3]
            ]);
            offset += 4;

            (raw_field_type & 0x7FFF, Some(enterprise_id))
        } else {
            (raw_field_type, None)
        };

        fields.push(TemplateField {
            field_type,
            field_length,
            enterprise_number,
        });
    }

    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn test_socket_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 2055)
    }

    fn test_template() -> Template {
        Template::new(
            256,
            vec![
                TemplateField {
                    field_type: 1,
                    field_length: 4,
                    enterprise_number: None,
                },
                TemplateField {
                    field_type: 7,
                    field_length: 2,
                    enterprise_number: None,
                },
            ],
        )
    }

    #[test]
    fn test_template_creation() {
        let template = test_template();
        assert_eq!(template.template_id, 256);
        assert_eq!(template.fields.len(), 2);
        assert_eq!(template.usage_count, 0);
        assert!(!template.is_expired(Duration::from_secs(1)));
    }

    #[test]
    fn test_template_record_size() {
        let template = test_template();
        assert_eq!(template.record_size(), Some(6)); // 4 + 2 bytes

        let variable_template = Template::new(
            257,
            vec![
                TemplateField {
                    field_type: 1,
                    field_length: 4,
                    enterprise_number: None,
                },
                TemplateField {
                    field_type: 2,
                    field_length: 65535, // Variable length
                    enterprise_number: None,
                },
            ],
        );
        assert_eq!(variable_template.record_size(), None);
        assert!(variable_template.has_variable_fields());
    }

    #[test]
    fn test_cache_operations() {
        let cache = TemplateCache::new(10);
        let key = (test_socket_addr(), 1, 256);
        let template = test_template();

        // Test miss
        assert!(cache.get(&key).is_none());
        
        // Test insert and hit
        cache.insert(key, template.clone());
        assert_eq!(cache.len(), 1);
        
        let retrieved = cache.get(&key).unwrap();
        assert_eq!(retrieved.template_id, template.template_id);
        assert_eq!(retrieved.usage_count, 1); // Should be incremented

        // Test stats
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.insertions, 1);
        assert_eq!(stats.current_size, 1);
    }

    #[test]
    fn test_cache_expiration() {
        let cache = TemplateCache::new(10);
        let key = (test_socket_addr(), 1, 256);
        let mut template = test_template();
        
        // Create an expired template
        template.last_used = Instant::now() - Duration::from_secs(7200); // 2 hours ago
        cache.insert(key, template);
        
        // Should find it before cleanup (but don't call get() as it updates last_used)
        let debug_templates = cache.debug_templates(10);
        assert!(!debug_templates.is_empty());
        
        // Debug: print templates before cleanup
        println!("Templates before cleanup: {:?}", debug_templates);
        // Cleanup with 1 hour timeout should remove it
        cache.cleanup_expired(3600);
        
        // Check that template was removed
        assert!(cache.get(&key).is_none());
        
        let stats = cache.stats();
        assert_eq!(stats.expired_removals, 1);
    }

    #[test]
    fn test_netflow_v9_template_parsing() {
        let mut data = vec![0u8; 12]; // Template ID + field count + 2 fields
        
        // Template ID = 256, Field count = 2
        data[0..2].copy_from_slice(&256u16.to_be_bytes());
        data[2..4].copy_from_slice(&2u16.to_be_bytes());
        
        // Field 1: type=1, length=4
        data[4..6].copy_from_slice(&1u16.to_be_bytes());
        data[6..8].copy_from_slice(&4u16.to_be_bytes());
        
        // Field 2: type=7, length=2
        data[8..10].copy_from_slice(&7u16.to_be_bytes());
        data[10..12].copy_from_slice(&2u16.to_be_bytes());
        
        let fields = parse_netflow_v9_template_fields(&data).unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].field_type, 1);
        assert_eq!(fields[0].field_length, 4);
        assert_eq!(fields[0].enterprise_number, None);
        assert_eq!(fields[1].field_type, 7);
        assert_eq!(fields[1].field_length, 2);
    }

    #[test]
    fn test_ipfix_enterprise_template_parsing() {
        let mut data = vec![0u8; 16]; // Template ID + field count + 1 enterprise field
        
        // Template ID = 256, Field count = 1
        data[0..2].copy_from_slice(&256u16.to_be_bytes());
        data[2..4].copy_from_slice(&1u16.to_be_bytes());
        
        // Enterprise field: type=0x8001 (enterprise bit set), length=4
        data[4..6].copy_from_slice(&0x8001u16.to_be_bytes());
        data[6..8].copy_from_slice(&4u16.to_be_bytes());
        
        // Enterprise ID = 9 (Cisco)
        data[8..12].copy_from_slice(&9u32.to_be_bytes());
        
        let fields = parse_ipfix_template_fields(&data).unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_type, 1); // Enterprise bit stripped
        assert_eq!(fields[0].field_length, 4);
        assert_eq!(fields[0].enterprise_number, Some(9));
    }

    #[test]
    fn test_invalid_template_data() {
        let short_data = vec![0u8; 2]; // Too short
        assert!(parse_netflow_v9_template_fields(&short_data).is_err());
        assert!(parse_ipfix_template_fields(&short_data).is_err());
        
        let mut incomplete_data = vec![0u8; 6]; // Header says 2 fields but only space for 1
        incomplete_data[2..4].copy_from_slice(&2u16.to_be_bytes()); // 2 fields
        assert!(parse_netflow_v9_template_fields(&incomplete_data).is_err());
    }

    #[test]
    fn test_cache_debug() {
        let cache = TemplateCache::new(10);
        let key = (test_socket_addr(), 1, 256);
        cache.insert(key, test_template());
        
        let debug_templates = cache.debug_templates(5);
        assert_eq!(debug_templates.len(), 1);
        assert_eq!(debug_templates[0].0, key);
        assert_eq!(debug_templates[0].1.template_id, 256);
    }
}