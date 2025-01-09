use std::sync::Arc;

use crate::sinks::Healthcheck;
use crate::{config::SinkContext, enrichment_tables::memory::Memory};
use async_trait::async_trait;
use futures::{future, FutureExt};
use tokio::sync::Mutex;
use vector_lib::config::{AcknowledgementsConfig, Input};
use vector_lib::enrichment::Table;
use vector_lib::{configurable::configurable_component, sink::VectorSink};

use crate::config::{EnrichmentTableConfig, SinkConfig};

use super::internal_events::MemoryTableInternalMetricsConfig;

/// Configuration for the `memory` enrichment table.
#[configurable_component(enrichment_table("memory"))]
#[derive(Clone)]
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
    /// Maximum size of the table in bytes. All insertions that would
    /// cause this table to grow over this size are rejected.
    ///
    /// By default, there is no size limit.
    #[serde(default = "default_max_byte_size")]
    pub max_byte_size: u64,

    /// Configuration of internal metrics
    #[configurable(derived)]
    #[serde(default)]
    pub internal_metrics: MemoryTableInternalMetricsConfig,

    #[serde(skip)]
    memory: Arc<Mutex<Option<Box<Memory>>>>,
}

impl PartialEq for MemoryConfig {
    fn eq(&self, other: &Self) -> bool {
        self.ttl == other.ttl
            && self.scan_interval == other.scan_interval
            && self.flush_interval == other.flush_interval
    }
}
impl Eq for MemoryConfig {}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            ttl: default_ttl(),
            scan_interval: default_scan_interval(),
            flush_interval: default_flush_interval(),
            memory: Arc::new(Mutex::new(None)),
            max_byte_size: default_max_byte_size(),
            internal_metrics: MemoryTableInternalMetricsConfig::default(),
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

const fn default_max_byte_size() -> u64 {
    0
}

impl MemoryConfig {
    async fn get_or_build_memory(&self) -> Memory {
        let mut boxed_memory = self.memory.lock().await;
        *boxed_memory
            .get_or_insert_with(|| Box::new(Memory::new(self.clone())))
            .clone()
    }
}

impl EnrichmentTableConfig for MemoryConfig {
    async fn build(
        &self,
        _globals: &crate::config::GlobalOptions,
    ) -> crate::Result<Box<dyn Table + Send + Sync>> {
        Ok(Box::new(self.get_or_build_memory().await))
    }

    fn sink_config(&self) -> Option<Box<dyn SinkConfig>> {
        Some(Box::new(self.clone()))
    }
}

#[async_trait]
#[typetag::serde(name = "memory_enrichment_table")]
impl SinkConfig for MemoryConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink = VectorSink::from_event_streamsink(self.get_or_build_memory().await);

        Ok((sink, future::ok(()).boxed()))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &AcknowledgementsConfig::DEFAULT
    }
}

impl std::fmt::Debug for MemoryConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryConfig")
            .field("ttl", &self.ttl)
            .field("scan_interval", &self.scan_interval)
            .field("flush_interval", &self.flush_interval)
            .field("max_byte_size", &self.max_byte_size)
            .finish()
    }
}

impl_generate_config_from_default!(MemoryConfig);
