use std::num::NonZeroU64;
use std::sync::Arc;

use crate::sinks::Healthcheck;
use crate::sources::Source;
use crate::{config::SinkContext, enrichment_tables::memory::Memory};
use async_trait::async_trait;
use futures::{future, FutureExt};
use tokio::sync::Mutex;
use vector_lib::config::{AcknowledgementsConfig, DataType, Input, LogNamespace};
use vector_lib::enrichment::Table;
use vector_lib::id::ComponentKey;
use vector_lib::schema::{self};
use vector_lib::{configurable::configurable_component, sink::VectorSink};
use vrl::path::OwnedTargetPath;
use vrl::value::Kind;

use crate::config::{EnrichmentTableConfig, SinkConfig, SourceConfig, SourceContext, SourceOutput};

use super::internal_events::InternalMetricsConfig;

/// Configuration for the `memory` enrichment table.
#[configurable_component(enrichment_table("memory"))]
#[derive(Clone)]
pub struct MemoryConfig {
    /// TTL (time-to-live in seconds) is used to limit the lifetime of data stored in the cache.
    /// When TTL expires, data behind a specific key in the cache is removed.
    /// TTL is reset when the key is replaced.
    #[serde(default = "default_ttl")]
    pub ttl: u64,
    /// The scan interval used to look for expired records. This is provided
    /// as an optimization to ensure that TTL is updated, but without doing
    /// too many cache scans.
    #[serde(default = "default_scan_interval")]
    pub scan_interval: NonZeroU64,
    /// The interval used for making writes visible in the table.
    /// Longer intervals might get better performance,
    /// but there is a longer delay before the data is visible in the table.
    /// Since every TTL scan makes its changes visible, only use this value
    /// if it is shorter than the `scan_interval`.
    ///
    /// By default, all writes are made visible immediately.
    #[serde(skip_serializing_if = "vector_lib::serde::is_default")]
    pub flush_interval: Option<u64>,
    /// Maximum size of the table in bytes. All insertions that make
    /// this table bigger than the maximum size are rejected.
    ///
    /// By default, there is no size limit.
    #[serde(skip_serializing_if = "vector_lib::serde::is_default")]
    pub max_byte_size: Option<u64>,
    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,
    /// Interval for dumping all data from the table when used as a source.
    ///
    /// By default, no dumps happen and this table can't be used as a source,
    /// unless a non-zero dump interval is provided
    #[serde(skip_serializing_if = "vector_lib::serde::is_default")]
    pub dump_interval: Option<u64>,
    /// Batch size for data dumping. Used to prevent dumping entire table at
    /// once and blocking the system.
    ///
    /// By default, batches are not used and entire table is dumped.
    #[serde(skip_serializing_if = "vector_lib::serde::is_default")]
    pub dump_batch_size: Option<u64>,
    /// If set to true, all data will be removed from cache after dumping.
    /// Only valid if used as a source and dump_interval > 0
    ///
    /// By default, dump will not remove data from cache
    #[serde(default = "crate::serde::default_false")]
    pub remove_after_dump: bool,
    /// Key to use for this component when used as a source.
    ///
    /// If this table is used as a source, this has to be defined and must be different from the
    /// component key.
    #[serde(skip_serializing_if = "vector_lib::serde::is_default")]
    pub source_key: Option<String>,
    /// Configuration of internal metrics
    #[configurable(derived)]
    #[serde(default)]
    pub internal_metrics: InternalMetricsConfig,

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
            flush_interval: None,
            memory: Arc::new(Mutex::new(None)),
            max_byte_size: None,
            log_namespace: None,
            dump_interval: None,
            dump_batch_size: None,
            remove_after_dump: false,
            source_key: None,
            internal_metrics: InternalMetricsConfig::default(),
        }
    }
}

const fn default_ttl() -> u64 {
    600
}

const fn default_scan_interval() -> NonZeroU64 {
    unsafe { NonZeroU64::new_unchecked(30) }
}

impl MemoryConfig {
    /// Just pub for now
    pub async fn get_or_build_memory(&self) -> Memory {
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

    fn sink_config(
        &self,
        default_key: &ComponentKey,
    ) -> Option<(ComponentKey, Box<dyn SinkConfig>)> {
        Some((default_key.clone(), Box::new(self.clone())))
    }

    fn source_config(
        &self,
        default_key: &ComponentKey,
    ) -> Option<(ComponentKey, Box<dyn SourceConfig>)> {
        if self.dump_interval.is_some() {
            Some((
                self.source_key
                    .clone()
                    .map(ComponentKey::from)
                    .unwrap_or(default_key.clone()),
                Box::new(self.clone()),
            ))
        } else {
            None
        }
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

#[async_trait]
#[typetag::serde(name = "memory_enrichment_table")]
impl SourceConfig for MemoryConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let memory = self.get_or_build_memory().await;

        let log_namespace = cx.log_namespace(self.log_namespace);

        Ok(Box::pin(
            memory.as_source(cx.shutdown, cx.out, log_namespace).run(),
        ))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        let schema_definition = match log_namespace {
            LogNamespace::Legacy => schema::Definition::default_legacy_namespace(),
            LogNamespace::Vector => {
                schema::Definition::new_with_default_metadata(Kind::any_object(), [log_namespace])
                    .with_meaning(OwnedTargetPath::event_root(), "message")
            }
        }
        .with_standard_vector_source_metadata();

        vec![SourceOutput::new_maybe_logs(
            DataType::Log,
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        false
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
