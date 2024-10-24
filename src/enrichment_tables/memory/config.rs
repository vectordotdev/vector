use crate::enrichment_tables::memory::Memory;
use vector_lib::configurable::configurable_component;
use vector_lib::enrichment::Table;

use crate::config::EnrichmentTableConfig;

/// Configuration for the `memory` enrichment table.
#[configurable_component(enrichment_table("memory"))]
#[derive(Clone, Eq, PartialEq)]
pub struct MemoryConfig {
    /// TTL (time-to-live), used to limit lifetime of data stored in cache.
    /// When TTL expires, data behind a specific key in cache is removed.
    /// TTL is reset when replacing the key.
    #[serde(default = "default_ttl")]
    pub ttl: u64,
    /// Scan interval for updating TTL of keys in seconds. This is provided
    /// as an optimization, to ensure that TTL is updated, but without doing
    /// too many cache scans.
    #[serde(default = "default_scan_interval")]
    pub scan_interval: u64,
    /// Interval for making writes visible in the table.
    /// Longer interval might get better performance,
    /// but data would be visible in the table after a longer delay.
    /// Since every TTL scan makes its changes visible, this value
    /// only makes sense if it is shorter than scan_interval
    ///
    /// By default, all writes are made visible immediately.
    #[serde(default = "default_flush_interval")]
    pub flush_interval: u64,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            ttl: default_ttl(),
            scan_interval: default_scan_interval(),
            flush_interval: default_flush_interval(),
        }
    }
}

const fn default_ttl() -> u64 {
    600
}

const fn default_scan_interval() -> u64 {
    30
}

const fn default_flush_interval() -> u64 {
    0
}

impl EnrichmentTableConfig for MemoryConfig {
    async fn build(
        &self,
        _globals: &crate::config::GlobalOptions,
    ) -> crate::Result<Box<dyn Table + Send + Sync>> {
        Ok(Box::new(Memory::new(self.clone())))
    }
}

impl std::fmt::Debug for MemoryConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryConfig")
            .field("ttl", &self.ttl)
            .field("scan_interval", &self.scan_interval)
            .field("flush_interval", &self.flush_interval)
            .finish()
    }
}

impl_generate_config_from_default!(MemoryConfig);
