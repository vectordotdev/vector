use std::{num::NonZeroU64, sync::Arc};

use async_trait::async_trait;
use futures::{FutureExt, future};
use tokio::sync::Mutex;
use vector_lib::{
    config::{AcknowledgementsConfig, DataType, Input, LogNamespace},
    configurable::configurable_component,
    enrichment::Table,
    id::ComponentKey,
    lookup::lookup_v2::OptionalValuePath,
    schema::{self},
    sink::VectorSink,
};
use vrl::{path::OwnedTargetPath, value::Kind};

use super::{Memory, internal_events::InternalMetricsConfig, source::EXPIRED_ROUTE};
use crate::{
    config::{
        EnrichmentTableConfig, SinkConfig, SinkContext, SourceConfig, SourceContext, SourceOutput,
        ValidatedSink,
    },
    enrichment_tables::memory::{
        bloom_table::{BloomMemoryConfig, BloomMemoryTable},
        cuckoo_table::{CuckooMemoryConfig, CuckooMemoryTable},
    },
    sinks::Healthcheck,
    sources::Source,
};

/// Configuration for the `memory` enrichment table.
#[configurable_component(enrichment_table("memory"))]
#[derive(Clone)]
#[serde(deny_unknown_fields)]
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
    /// NOTE: For cuckoo filter, all writes are visible immediately. Flush interval still defines
    /// when metrics for cuckoo filter are made visible.
    ///
    /// By default, all writes are made visible immediately.
    #[serde(skip_serializing_if = "vector_lib::serde::is_default")]
    pub flush_interval: Option<NonZeroU64>,
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
    /// Configuration of internal metrics
    #[serde(default)]
    pub internal_metrics: InternalMetricsConfig,
    /// Configuration for source functionality.
    #[serde(skip_serializing_if = "vector_lib::serde::is_default")]
    pub source_config: Option<MemorySourceConfig>,
    /// Field in the incoming value used as the TTL override.
    #[serde(default)]
    pub ttl_field: OptionalValuePath,
    /// Behavior for memory table state on configuration reload.
    #[serde(default)]
    pub reload_behavior: ReloadBehavior,

    /// Set to make the table act as a probabilistic filter instead of storing original values. This
    /// will prevent reading values from the table - found keys will have empty value.
    #[serde(default)]
    pub filter: Option<TableFilter>,

    #[serde(skip)]
    memory: Arc<Mutex<Option<Box<Memory>>>>,
    #[serde(skip)]
    cuckoo: Arc<Mutex<Option<Box<CuckooMemoryTable>>>>,
    #[serde(skip)]
    bloom: Arc<Mutex<Option<Box<BloomMemoryTable>>>>,
}

/// Behavior for memory enrichment table state on configuration reload.
#[configurable_component]
#[derive(Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ReloadBehavior {
    /// Always clear state on configuration reload.
    #[default]
    ClearState,
    /// Try to preserve state when possible.
    PreserveState,
}

/// Configuration for memory enrichment table source functionality.
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MemorySourceConfig {
    /// Interval for exporting all data from the table when used as a source.
    #[serde(skip_serializing_if = "vector_lib::serde::is_default")]
    pub export_interval: Option<NonZeroU64>,
    /// Batch size for data exporting. Used to prevent exporting entire table at
    /// once and blocking the system.
    ///
    /// By default, batches are not used and entire table is exported.
    #[serde(skip_serializing_if = "vector_lib::serde::is_default")]
    pub export_batch_size: Option<u64>,
    /// If set to true, all data will be removed from cache after exporting.
    /// Only valid if used as a source and export_interval > 0
    ///
    /// By default, export will not remove data from cache
    #[serde(default = "crate::serde::default_false")]
    pub remove_after_export: bool,
    /// Set to true to export expired items via the `expired` output port.
    /// Expired items ignore other settings and are exported as they are flushed from the table.
    #[serde(default = "crate::serde::default_false")]
    pub export_expired_items: bool,
    /// Key to use for this component when used as a source. This must be different from the
    /// component key.
    pub source_key: String,
}

/// Configuration for memory enrichment table filter functionality.
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "type")]
#[configurable(metadata(docs::enum_tag_description = "The probabilistic filter to use."))]
pub enum TableFilter {
    /// Cuckoo filter
    ///
    /// Supports removal by accepting null values for keys, as well as TTL and LRU.
    Cuckoo(CuckooMemoryConfig),
    /// Bloom filter
    ///
    /// Only supports insertion and presence check, no TTL
    Bloom(BloomMemoryConfig),
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
            cuckoo: Arc::new(Mutex::new(None)),
            bloom: Arc::new(Mutex::new(None)),
            max_byte_size: None,
            log_namespace: None,
            source_config: None,
            internal_metrics: InternalMetricsConfig::default(),
            ttl_field: OptionalValuePath::none(),
            reload_behavior: Default::default(),
            filter: None,
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
    pub(super) async fn get_or_build_memory(
        &self,
        prev_state: Option<Box<dyn std::any::Any + Send + Sync>>,
    ) -> Memory {
        let mut boxed_memory = self.memory.lock().await;
        *boxed_memory
            .get_or_insert_with(|| {
                if let Some(prev) = prev_state {
                    Box::new(Memory::from_previous_state(self.clone(), prev))
                } else {
                    Box::new(Memory::new(self.clone()))
                }
            })
            .clone()
    }

    pub(super) async fn get_or_build_cuckoo(
        &self,
        prev_state: Option<Box<dyn std::any::Any + Send + Sync>>,
    ) -> crate::Result<CuckooMemoryTable> {
        let Some(TableFilter::Cuckoo(cuckoo)) = &self.filter else {
            panic!("No cuckoo");
        };
        let mut boxed_cuckoo = self.cuckoo.lock().await;
        if let Some(boxed_cuckoo) = boxed_cuckoo.as_ref() {
            Ok(*boxed_cuckoo.clone())
        } else {
            Ok(*boxed_cuckoo
                .insert(if let Some(prev) = prev_state {
                    Box::new(CuckooMemoryTable::from_previous_state(
                        self.clone(),
                        cuckoo.clone(),
                        prev,
                    )?)
                } else {
                    Box::new(CuckooMemoryTable::new(self.clone(), cuckoo.clone())?)
                })
                .clone())
        }
    }

    pub(super) async fn get_or_build_bloom(
        &self,
        prev_state: Option<Box<dyn std::any::Any + Send + Sync>>,
    ) -> crate::Result<BloomMemoryTable> {
        let mut boxed_bloom = self.bloom.lock().await;
        let Some(TableFilter::Bloom(bloom)) = &self.filter else {
            panic!("No bloom");
        };
        if let Some(boxed_bloom) = boxed_bloom.as_ref() {
            Ok(*boxed_bloom.clone())
        } else {
            Ok(*boxed_bloom
                .insert(if let Some(prev) = prev_state {
                    Box::new(BloomMemoryTable::from_previous_state(
                        self.clone(),
                        bloom.clone(),
                        prev,
                    )?)
                } else {
                    Box::new(BloomMemoryTable::new(self.clone(), bloom.clone())?)
                })
                .clone())
        }
    }

    fn validate_filter(&self) -> crate::Result<()> {
        match &self.filter {
            Some(TableFilter::Cuckoo(_)) if self.source_config.is_some() => {
                return Err("Source functionality is not supported for cuckoo filter".into());
            }
            Some(TableFilter::Bloom(_)) if self.source_config.is_some() => {
                return Err("Source functionality is not supported for bloom filter".into());
            }
            Some(TableFilter::Bloom(_))
                if self.ttl_field.path.is_some() || self.ttl != default_ttl() =>
            {
                return Err("TTL functionality is not supported for bloom filter.".into());
            }
            Some(TableFilter::Bloom(_)) if self.scan_interval != default_scan_interval() => {
                return Err("`scan_interval` has no effect for bloom filter.".into());
            }
            Some(TableFilter::Bloom(bloom)) => {
                let filter_size = bloom.filter_size();
                if let Some(max_byte_size) = self.max_byte_size
                    && filter_size > max_byte_size
                {
                    return Err(format!("Configured bloom filter is larger ({}) than defined `max_byte_size` ({}). Reduce the size of bloom filter or increase or remove `max_byte_size`.", filter_size, max_byte_size).into());
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl EnrichmentTableConfig for MemoryConfig {
    async fn build(
        &self,
        _globals: &crate::config::GlobalOptions,
        prev_state: Option<Box<dyn std::any::Any + Send + Sync>>,
    ) -> crate::Result<Box<dyn Table + Send + Sync>> {
        self.validate_filter()?;
        match &self.filter {
            Some(TableFilter::Cuckoo(_)) => {
                Ok(Box::new(self.get_or_build_cuckoo(prev_state).await?))
            }
            Some(TableFilter::Bloom(_)) => Ok(Box::new(self.get_or_build_bloom(prev_state).await?)),
            None => Ok(Box::new(self.get_or_build_memory(prev_state).await)),
        }
    }

    fn wants_previous_state(&self) -> bool {
        matches!(self.reload_behavior, ReloadBehavior::PreserveState)
    }

    fn sink_config(
        &self,
        default_key: &ComponentKey,
    ) -> Option<(ComponentKey, Box<dyn SinkConfig>)> {
        Some((default_key.clone(), Box::new(self.clone())))
    }

    fn source_config(
        &self,
        _default_key: &ComponentKey,
    ) -> Option<(ComponentKey, Box<dyn SourceConfig>)> {
        let Some(source_config) = &self.source_config else {
            return None;
        };
        // Filters can't be used as a source
        if self.filter.is_some() {
            return None;
        }
        Some((
            source_config.source_key.clone().into(),
            Box::new(self.clone()),
        ))
    }
}

#[async_trait]
#[typetag::serde(name = "memory_enrichment_table")]
impl SinkConfig for MemoryConfig {
    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &AcknowledgementsConfig::DEFAULT
    }
}

#[async_trait]
impl ValidatedSink for MemoryConfig {
    type Validated = ();

    fn validate(&self) -> crate::Result<Self::Validated> {
        self.validate_filter()?;
        Ok(())
    }

    async fn build(
        &self,
        _validated: &Self::Validated,
        _cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink = match &self.filter {
            Some(TableFilter::Cuckoo(_)) => {
                VectorSink::from_event_streamsink(self.get_or_build_cuckoo(None).await?)
            }
            Some(TableFilter::Bloom(_)) => {
                VectorSink::from_event_streamsink(self.get_or_build_bloom(None).await?)
            }
            None => VectorSink::from_event_streamsink(self.get_or_build_memory(None).await),
        };

        Ok((sink, future::ok(()).boxed()))
    }
}

#[async_trait]
#[typetag::serde(name = "memory_enrichment_table")]
impl SourceConfig for MemoryConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let memory = self.get_or_build_memory(None).await;

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

        if self
            .source_config
            .as_ref()
            .map(|c| c.export_expired_items)
            .unwrap_or_default()
        {
            vec![
                SourceOutput::new_maybe_logs(DataType::Log, schema_definition.clone()),
                SourceOutput::new_maybe_logs(DataType::Log, schema_definition)
                    .with_port(EXPIRED_ROUTE),
            ]
        } else {
            vec![SourceOutput::new_maybe_logs(
                DataType::Log,
                schema_definition,
            )]
        }
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

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use crate::config::ValidatedSink;
    use crate::enrichment_tables::memory::bloom_table::BloomMemoryConfig;

    use super::*;

    fn bloom_filter() -> TableFilter {
        TableFilter::Bloom(BloomMemoryConfig {
            max_entries: NonZeroUsize::new(1000).unwrap(),
        })
    }

    #[test]
    fn validate_rejects_incompatible_filter_combinations() {
        // Source functionality is not supported for bloom filter.
        let config = MemoryConfig {
            filter: Some(bloom_filter()),
            source_config: Some(MemorySourceConfig {
                export_interval: None,
                export_batch_size: None,
                remove_after_export: false,
                export_expired_items: false,
                source_key: "source".to_string(),
            }),
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // TTL functionality is not supported for bloom filter.
        let config = MemoryConfig {
            filter: Some(bloom_filter()),
            ttl_field: OptionalValuePath::new("ttl"),
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // `scan_interval` has no effect for bloom filter.
        let config = MemoryConfig {
            filter: Some(bloom_filter()),
            scan_interval: NonZeroU64::new(60).unwrap(),
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // A bloom filter larger than `max_byte_size` fails validation.
        let config = MemoryConfig {
            filter: Some(bloom_filter()),
            max_byte_size: Some(1),
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // A valid config passes validation.
        let config = MemoryConfig::default();
        assert!(config.validate().is_ok());
    }
}
