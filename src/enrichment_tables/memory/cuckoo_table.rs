use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{BufReader, BufWriter, Write},
    num::{NonZeroU64, NonZeroUsize},
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

use async_trait::async_trait;
use bytes::Bytes;
use cuckoo_clock::{
    CuckooFilter, ExportableRandomState, InsertValues, LookupValues,
    config::{CounterConfig, CuckooConfiguration, LruAgingStrategy, LruConfig, TtlConfig},
};
use futures::{
    Stream, StreamExt,
    stream::{self, BoxStream},
};
use tempfile::NamedTempFile;
use tokio::{
    task::JoinSet,
    time::{Instant, interval, interval_at},
};
use tokio_stream::wrappers::IntervalStream;
use tracing::Instrument;
use vector_config::configurable_component;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    enrichment::{Case, Condition, Error, IndexHandle, Table},
    event::{Event, EventStatus, Finalizable},
    internal_event::{
        ByteSize, BytesSent, CountByteSize, EventsSent, InternalEventHandle, Output, Protocol,
    },
    lookup::lookup_v2::OptionalValuePath,
    sink::StreamSink,
};
use vrl::value::{KeyString, ObjectMap, Value};

use crate::enrichment_tables::memory::{
    MemoryConfig,
    internal_events::{
        MemoryEnrichmentTableFlushed, MemoryEnrichmentTableInsertFailed,
        MemoryEnrichmentTableInserted, MemoryEnrichmentTableRead, MemoryEnrichmentTableReadFailed,
        MemoryEnrichmentTableRemoved, MemoryEnrichmentTableTtlExpiredCount,
    },
};

/// A struct that implements [vector_lib::enrichment::Table] to handle loading enrichment data from a cuckoo table.
#[derive(Clone)]
pub(super) struct CuckooMemoryTable {
    filter: CuckooFilter<ExportableRandomState>,
    pub(super) config: MemoryConfig,
    cuckoo_config: CuckooMemoryConfig,
}

/// Configuration of cuckoo filter for memory table.
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CuckooMemoryConfig {
    /// Number of bits used for fingerprint.
    #[serde(default = "default_cuckoo_fingerprint_bits")]
    pub fingerprint_bits: NonZeroUsize,
    /// Number of slots in each bucket
    #[serde(default = "default_cuckoo_bucket_size")]
    pub bucket_size: NonZeroUsize,
    /// Maximum number of entries that can be stored in the filter (actual capacity will usually be
    /// larger)
    pub max_entries: usize,
    /// Max number of kicks when experiencing hash collisions.
    #[serde(default = "default_cuckoo_max_kicks")]
    pub max_kicks: usize,
    /// Can be set to true to use LRU strategy for kicking.
    #[serde(default = "crate::serde::default_false")]
    pub lru_enabled: bool,
    /// Can be set to true to delete unused items on scan when LRU is used.
    #[serde(default = "crate::serde::default_false")]
    pub lru_deletion_enabled: bool,
    /// Number of bits to use to track LRU counter.
    /// Low bit count will reduce the maximum LRU counter value, making the items expire sooner if
    /// unused.
    #[serde(default = "default_cuckoo_lru_bits")]
    pub lru_bits: NonZeroUsize,
    /// Starting value for LRU counter on item insertion.
    /// Higher value will give newer items a higher probability to stay in the filter.
    #[serde(default = "default_cuckoo_lru_starting_value")]
    pub lru_starting_value: u32,
    /// Value to increase LRU counter by on each item access.
    #[serde(default = "default_cuckoo_lru_increment")]
    pub lru_increment: u32,
    /// Strategy to use when aging LRU counters at each scan.
    #[serde(default)]
    pub lru_aging_strategy: CuckooLruAgingStrategy,
    /// Can be set to true to also track TTL for entries.
    #[serde(default = "crate::serde::default_true")]
    pub ttl_enabled: bool,
    /// Number of bits to use to track TTL. Low bit count will reduce maximum TTL and also require a
    /// worse resolution to keep working.
    #[serde(default = "default_cuckoo_ttl_bits")]
    pub ttl_bits: NonZeroUsize,
    /// Can be set to true to track a count alongside hashes.
    #[serde(default = "crate::serde::default_false")]
    pub counter_enabled: bool,
    /// Number of bits to use to track counter. This will limit the max value.
    #[serde(default = "default_cuckoo_counter_bits")]
    pub counter_bits: NonZeroUsize,
    /// Field in the incoming value used as the counter increment override.
    #[configurable(derived)]
    #[serde(default)]
    pub counter_field: OptionalValuePath,
    /// The amount to increment the counter by on every insertion. Negative values are allowed.
    #[serde(default = "default_cuckoo_counter_insertion_increment")]
    pub counter_insertion_increment: i32,
    /// The amount to increment the counter by on every lookup. Negative values are allowed.
    #[serde(default = "default_cuckoo_counter_lookup_increment")]
    pub counter_lookup_increment: i32,
    /// Path to the file to export data to periodically and on exit.
    /// Data will be imported from this file on startup and reload.
    ///
    /// If table `reload_behavior` is set to `clear-state` and this is set, the persisted state will
    /// still be read after reload.
    #[configurable(derived)]
    #[serde(default)]
    pub persistence_path: Option<PathBuf>,
    /// The interval used for exporting data.
    ///
    /// By default, export is only done on exit.
    #[serde(skip_serializing_if = "vector_lib::serde::is_default")]
    pub export_interval: Option<NonZeroU64>,
    /// Number of threads to use for scanning and updating LRU/TTL.
    ///
    /// By default, scanning is single threaded.
    #[serde(default)]
    pub scanning_threads: Option<NonZeroUsize>,
    /// If set to true scanning will not block insertions.
    /// This may affect behavior since blocking scans would free up space before insertions.
    ///
    /// By default, scanning is blocking.
    #[serde(default = "crate::serde::default_false")]
    pub concurrent_scanning: bool,
}

/// Aging strategy for LRU counters in cuckoo filters.
#[configurable_component]
#[derive(Clone, Default, Debug, PartialEq, Eq)]
#[serde(tag = "strategy", rename_all = "snake_case")]
#[configurable(metadata(docs::enum_tag_description = "The LRU aging strategy to use."))]
pub enum CuckooLruAgingStrategy {
    /// Aging LRU counters by halving their value on each scan.
    #[default]
    Halving,
    /// Aging LRU counters by decrementing by a fixed value on each scan.
    Decrement {
        /// Value to decrement by
        value: u32,
    },
}

impl From<&CuckooLruAgingStrategy> for LruAgingStrategy {
    fn from(value: &CuckooLruAgingStrategy) -> Self {
        match value {
            CuckooLruAgingStrategy::Halving => LruAgingStrategy::Halving,
            CuckooLruAgingStrategy::Decrement { value } => LruAgingStrategy::Decrement(*value),
        }
    }
}

const fn default_cuckoo_fingerprint_bits() -> NonZeroUsize {
    unsafe { NonZeroUsize::new_unchecked(8) }
}

const fn default_cuckoo_bucket_size() -> NonZeroUsize {
    unsafe { NonZeroUsize::new_unchecked(4) }
}

const fn default_cuckoo_ttl_bits() -> NonZeroUsize {
    unsafe { NonZeroUsize::new_unchecked(8) }
}

const fn default_cuckoo_lru_bits() -> NonZeroUsize {
    unsafe { NonZeroUsize::new_unchecked(8) }
}

const fn default_cuckoo_lru_starting_value() -> u32 {
    1
}

const fn default_cuckoo_lru_increment() -> u32 {
    1
}

const fn default_cuckoo_counter_bits() -> NonZeroUsize {
    unsafe { NonZeroUsize::new_unchecked(8) }
}

const fn default_cuckoo_counter_insertion_increment() -> i32 {
    1
}

const fn default_cuckoo_counter_lookup_increment() -> i32 {
    1
}

const fn default_cuckoo_max_kicks() -> usize {
    500
}

impl CuckooMemoryTable {
    /// Creates a new [CuckooMemoryTable] based on the provided config.
    pub(super) fn new(
        config: MemoryConfig,
        cuckoo_config: CuckooMemoryConfig,
    ) -> crate::Result<Self> {
        let built_config = Self::build_config(&config, &cuckoo_config)?;

        let filter = 'import: {
            if let Some(path) = &cuckoo_config.persistence_path {
                let file = match File::open(path) {
                    Ok(file) => file,
                    Err(err) => match err.kind() {
                        std::io::ErrorKind::NotFound => {
                            if let Some(parent) = path.parent()
                                && parent != ""
                                && !fs::metadata(parent).is_ok_and(|m| m.is_dir())
                            {
                                return Err(format!(
                                    "Cuckoo filter persistence path directory ({}) doesn't exist. This will prevent exporting the cuckoo filter state. Fix the `persistence_path` to ensure export works.",
                                    parent.to_str().unwrap_or(""),
                                )
                                .into());
                            }
                            break 'import CuckooFilter::new_random_exportable(built_config);
                        }
                        _ => {
                            return Err(format!(
                                "Couldn't open \"{}\" for cuckoo filter state import. {}",
                                path.to_str().unwrap_or(""),
                                err
                            )
                            .into());
                        }
                    },
                };
                let mut reader = BufReader::new(file);
                let (hasher, persisted_config) =
                    match CuckooFilter::<ExportableRandomState>::import_config(&mut reader) {
                        Ok(imported) => imported,
                        Err(error) => {
                            return Err(
                            format!("Cuckoo filter state import failed: {}. Delete the persisted state file ({}) to proceed.", error, path.to_str().unwrap_or("")).into(),
                        );
                        }
                    };

                if !built_config.compatible_layout(&persisted_config) {
                    return Err(
                        format!("Stored cuckoo filter configuration is not compatible with new configuration. Only changes to values that don't affect layout or size are allowed. If this is intended, remove the persisted state file ({}). Built: {:?}. Persisted: {:?}", path.to_str().unwrap_or(""), built_config, persisted_config).into(),
                    );
                }

                if let Some(ttl) = built_config.ttl_config()
                    && let Some(persisted_ttl) = persisted_config.ttl_config()
                    && ttl.ttl != persisted_ttl.ttl
                {
                    warn!(
                        "Persisted configuration had a different default TTL value ({}), comapared to the new value ({}). Previous default TTL value is effectively {} seconds, while the new one is {} seconds.",
                        persisted_ttl.ttl,
                        ttl.ttl,
                        (persisted_ttl.ttl.get() as u64) * config.scan_interval.get(),
                        config.ttl
                    );
                }

                match CuckooFilter::import_state(hasher, built_config, &mut reader) {
                    Ok(filter) => filter,
                    Err(error) => {
                        return Err(
                            format!("Cuckoo filter state import failed: {}. Delete the persisted state file ({}) to proceed.", error, path.to_str().unwrap_or("")).into(),
                        );
                    }
                }
            } else {
                CuckooFilter::new_random_exportable(built_config)
            }
        };

        Ok(Self {
            config,
            filter,
            cuckoo_config,
        })
    }

    /// Creates a new [CuckooMemoryTable] based on the provided config and previous state.
    pub(super) fn from_previous_state(
        config: MemoryConfig,
        cuckoo_config: CuckooMemoryConfig,
        prev_state: Box<dyn std::any::Any + Send + Sync>,
    ) -> crate::Result<Self> {
        if let Ok(prev_memory) = prev_state.downcast::<CuckooMemoryTable>() {
            if let Some(path) = &cuckoo_config.persistence_path
                && let Err(err) = File::open(path)
                && err.kind() == std::io::ErrorKind::NotFound
                && let Some(parent) = path.parent()
                && parent != ""
                && !fs::metadata(parent).is_ok_and(|m| m.is_dir())
            {
                return Err(format!(
                    "Cuckoo filter persistence path directory ({}) doesn't exist. This will prevent exporting the cuckoo filter state. Fix the `persistence_path` to ensure export works.",
                    parent.to_str().unwrap_or(""),
                )
                    .into());
            }
            let built_config = Self::build_config(&config, &cuckoo_config)?;
            let built_ttl = built_config.ttl_config().clone();
            if built_config.compatible_layout(&prev_memory.filter.get_configuration())
                && let Ok(mut old_filter) =
                    prev_memory.filter.exporter().snapshot().map(VecDeque::from)
                && let Ok((hasher, old_conf)) = CuckooFilter::import_config(&mut old_filter)
                && let Ok(filter) =
                    CuckooFilter::import_state(hasher, built_config, &mut old_filter)
            {
                if let Some(ttl) = built_ttl
                    && let Some(old_ttl) = old_conf.ttl_config()
                    && ttl.ttl != old_ttl.ttl
                {
                    warn!(
                        "Restored configuration had a different default TTL value ({}), comapared to the new value ({}). Previous default TTL value is effectively {} seconds, while the new one is {} seconds.",
                        old_ttl.ttl,
                        ttl.ttl,
                        (old_ttl.ttl.get() as u64) * config.scan_interval.get(),
                        config.ttl
                    );
                }
                return Ok(Self {
                    filter,
                    config,
                    cuckoo_config,
                });
            }
        }

        Self::new(config, cuckoo_config)
    }

    fn build_config(
        config: &MemoryConfig,
        cuckoo_config: &CuckooMemoryConfig,
    ) -> crate::Result<CuckooConfiguration> {
        let ttl_val = (config.ttl.div_ceil(config.scan_interval.get())).max(1);
        let mut builder = CuckooConfiguration::builder(cuckoo_config.max_entries)
            .fingerprint_bits(cuckoo_config.fingerprint_bits.get().try_into()?)
            .bucket_size(cuckoo_config.bucket_size)
            .max_kicks(cuckoo_config.max_kicks);

        if cuckoo_config.lru_enabled {
            let starting_value_needed_bits = cuckoo_config
                .lru_starting_value
                .checked_ilog2()
                .unwrap_or(0)
                + 1;
            if starting_value_needed_bits as usize > cuckoo_config.lru_bits.get() {
                return Err(format!(
                    "`lru_bits` ({}) must be set to at least {} to support the `lru_starting_value` value ({}).",
                    cuckoo_config.lru_bits.get(),
                    starting_value_needed_bits,
                    cuckoo_config.lru_starting_value,
                ).into());
            }
            let increment_needed_bits =
                cuckoo_config.lru_increment.checked_ilog2().unwrap_or(0) + 1;
            if increment_needed_bits as usize > cuckoo_config.lru_bits.get() {
                return Err(format!(
                    "`lru_bits` ({}) must be set to at least {} to support the `lru_increment` value ({}).",
                    cuckoo_config.lru_bits.get(),
                    increment_needed_bits,
                    cuckoo_config.lru_increment,
                ).into());
            }
            builder = builder.with_lru(LruConfig {
                counter_bits: cuckoo_config.lru_bits.get().try_into()?,
                remove_on_zero: cuckoo_config.lru_deletion_enabled,
                starting_value: cuckoo_config.lru_starting_value,
                increment: cuckoo_config.lru_increment,
                aging_strategy: (&cuckoo_config.lru_aging_strategy).into(),
            });
        }

        if cuckoo_config.ttl_enabled {
            let ttl_val: u32 = u32::try_from(ttl_val)?;
            let needed_bits = ttl_val.checked_ilog2().unwrap_or(0) + 1;
            if needed_bits as usize > cuckoo_config.ttl_bits.get() {
                return Err(
                    format!(
                    "`ttl_bits` ({}) must be set to at least {} to support the default `ttl` value ({}) at the configured scan interval ({}).",
                    cuckoo_config.ttl_bits.get(),
                        needed_bits,
                    config.ttl,
                    config.scan_interval.get()).into(),
                );
            }
            builder = builder.with_ttl(TtlConfig {
                ttl: ttl_val.try_into()?,
                ttl_bits: cuckoo_config.ttl_bits.get().try_into()?,
            });
        }

        if cuckoo_config.counter_enabled {
            builder = builder.with_counter(CounterConfig {
                counter_bits: cuckoo_config.counter_bits.get().try_into()?,
                change_on_insert: cuckoo_config.counter_insertion_increment,
                change_on_lookup: cuckoo_config.counter_lookup_increment,
            });
        }

        let built_config = builder.build()?;

        let filter_size = built_config.get_configured_memory_usage();
        if let Some(max_byte_size) = config.max_byte_size
            && filter_size as u64 > max_byte_size
        {
            return Err(format!("Configured cuckoo filter is larger ({}) than defined `max_byte_size` ({}). Reduce the size of cuckoo filter or increase or remove `max_byte_size`.", filter_size, max_byte_size).into());
        }

        Ok(built_config)
    }

    fn export(&self) {
        if let Some(path) = &self.cuckoo_config.persistence_path {
            let mut parent = path.clone();
            if parent.pop() {
                if parent == *"" {
                    parent = ".".into();
                }
                match NamedTempFile::new_in(parent) {
                    Ok(temp) => {
                        {
                            let mut writer = BufWriter::new(temp.as_file());
                            if self.export_to(&mut writer).is_err() {
                                return;
                            }
                        }
                        if let Err(error) = temp.persist(path) {
                            warn!("Cuckoo filter export failed: {}", error);
                        }
                    }
                    Err(err) => warn!(
                        "Couldn't open temporary file for export. Aborting export. Error: {}",
                        err
                    ),
                }
            }
        }
    }

    async fn scan(&self, scans_in_progress: &Arc<AtomicUsize>) {
        let mut handles = JoinSet::new();
        let filter = self.filter.clone();
        let count = self
            .cuckoo_config
            .scanning_threads
            .unwrap_or(NonZeroUsize::new(1).unwrap());
        scans_in_progress.fetch_add(count.get(), Ordering::AcqRel);
        for i in 0..count.get() {
            let filter = filter.clone();
            let scans_in_progress = Arc::clone(scans_in_progress);
            let lru_deletion_enabled = self.cuckoo_config.lru_deletion_enabled;
            let task = async move {
                let expired = if lru_deletion_enabled {
                    filter.scan_and_update_lru_partition(count, i);
                    // Run TTL scan separately when LRU deletion is enabled, to ensure
                    // correct TTL expired count
                    filter.scan_and_update_ttl_partition(count, i)
                } else {
                    filter.scan_and_update_full_partition(count, i)
                };
                emit!(MemoryEnrichmentTableTtlExpiredCount {
                    count: expired as u64
                });
                scans_in_progress.fetch_sub(1, Ordering::AcqRel);
            }
            .in_current_span();
            handles.spawn(task);
        }
        if !self.cuckoo_config.concurrent_scanning {
            let _ = handles.join_all().await;
            emit!(MemoryEnrichmentTableFlushed {
                new_objects_count: filter.get_item_count(),
                new_byte_size: filter.get_memory_usage()
            });
        } else {
            tokio::spawn(async move {
                let _ = handles.join_all().await;
                emit!(MemoryEnrichmentTableFlushed {
                    new_objects_count: filter.get_item_count(),
                    new_byte_size: filter.get_memory_usage()
                });
            });
        }
    }

    fn export_to(&self, mut writer: impl Write) -> Result<(), ()> {
        match self.filter.exporter().write_to(&mut writer) {
            Ok(()) => {
                if let Err(error) = writer.flush() {
                    warn!("Cuckoo filter export failed: {}", error);
                    return Err(());
                };
                Ok(())
            }
            Err(error) => {
                warn!("Cuckoo filter export failed: {}", error);
                Err(())
            }
        }
    }

    fn handle_value(&self, value: ObjectMap) {
        for (k, value) in value.iter() {
            if matches!(value, Value::Null) {
                if self.filter.remove(k) {
                    emit!(MemoryEnrichmentTableRemoved {
                        key: k,
                        include_key_metric_tag: self.config.internal_metrics.include_key_tag
                    });
                }

                continue;
            };

            let res = if self.cuckoo_config.ttl_enabled || self.cuckoo_config.counter_enabled {
                let mut ttl = self
                    .config
                    .ttl_field
                    .path
                    .as_ref()
                    .and_then(|p| value.get(p))
                    .and_then(|v| v.as_integer())
                    .and_then(|v| u64::try_from(v).ok())
                    .or(Some(self.config.ttl))
                    .map(|v| (v.div_ceil(self.config.scan_interval.get())).max(1))
                    .map(|v| u32::try_from(v).unwrap_or(u32::MAX));
                if let Some(ttl) = &mut ttl {
                    let needed_bits = ttl.checked_ilog2().unwrap_or(0) + 1;
                    if needed_bits as usize > self.cuckoo_config.ttl_bits.get() {
                        warn!(
                            "`ttl_bits` ({}) must be set to at least {} to support the provided `ttl` value ({}) at the configured scan interval ({}).",
                            self.cuckoo_config.ttl_bits.get(),
                            needed_bits,
                            ttl,
                            self.config.scan_interval.get()
                        );
                        // Unchecked conversion to u32, because ttl_bits can't be higher than 32 anyways
                        *ttl = 2_u32
                            .checked_pow(self.cuckoo_config.ttl_bits.get() as u32)
                            .map(|ttl| ttl - 1)
                            .unwrap_or(u32::MAX);
                    }
                }
                let counter = self
                    .cuckoo_config
                    .counter_field
                    .path
                    .as_ref()
                    .and_then(|p| value.get(p))
                    .and_then(|v| v.as_integer())
                    .map(|v| {
                        i32::try_from(v)
                            .ok()
                            .unwrap_or_else(|| v.clamp(i32::MIN as i64, i32::MAX as i64) as i32)
                    })
                    .unwrap_or(self.cuckoo_config.counter_insertion_increment);
                self.filter.insert_if_not_present_with_update(
                    k,
                    InsertValues {
                        ttl,
                        counter: Some(counter),
                    },
                    LookupValues {
                        ttl,
                        counter_diff: Some(counter),
                    },
                )
            } else {
                self.filter.insert_if_not_present(k)
            };

            if res.is_some_and(|r| r.matches_key(k, &self.filter)) {
                emit!(MemoryEnrichmentTableInsertFailed {
                    key: k,
                    include_key_metric_tag: self.config.internal_metrics.include_key_tag
                });
            } else {
                emit!(MemoryEnrichmentTableInserted {
                    key: k,
                    include_key_metric_tag: self.config.internal_metrics.include_key_tag
                });
            }
        }
    }
}

impl Table for CuckooMemoryTable {
    fn find_table_row<'a>(
        &self,
        case: Case,
        condition: &'a [Condition<'a>],
        select: Option<&'a [String]>,
        wildcard: Option<&Value>,
        index: Option<IndexHandle>,
    ) -> Result<ObjectMap, Error> {
        let mut rows = self.find_table_rows(case, condition, select, wildcard, index)?;

        match rows.pop() {
            Some(row) if rows.is_empty() => Ok(row),
            Some(_) => Err(Error::MoreThanOneRowFound),
            None => Err(Error::NoRowsFound),
        }
    }

    fn find_table_rows<'a>(
        &self,
        _case: Case,
        condition: &'a [Condition<'a>],
        _select: Option<&'a [String]>,
        _wildcard: Option<&Value>,
        _index: Option<IndexHandle>,
    ) -> Result<Vec<ObjectMap>, Error> {
        match condition.first() {
            Some(_) if condition.len() > 1 => Err(Error::OnlyOneConditionAllowed),
            Some(Condition::Equals { value, .. }) => {
                let key = value.to_string_lossy();
                if let Some(associated_data) = self.filter.get_associated_data(&key) {
                    emit!(MemoryEnrichmentTableRead {
                        key: &key,
                        include_key_metric_tag: self.config.internal_metrics.include_key_tag
                    });
                    let mut result = ObjectMap::from([
                        (
                            KeyString::from("key"),
                            Value::Bytes(Bytes::copy_from_slice(key.as_bytes())),
                        ),
                        (
                            KeyString::from("fingerprint"),
                            Value::Bytes(Bytes::from(format!(
                                "{:X}",
                                associated_data.get_fingerprint()
                            ))),
                        ),
                        (KeyString::from("value"), Value::Null),
                    ]);
                    if let Ok(ttl) = associated_data.get_stored_ttl_value()
                        && let Ok(ttl) = (ttl as u64 * self.config.scan_interval.get()).try_into()
                    {
                        result.insert(KeyString::from("ttl"), Value::Integer(ttl));
                    }
                    if let Ok(counter) = associated_data.get_counter() {
                        result.insert(KeyString::from("counter"), Value::Integer(counter.into()));
                    }
                    Ok(vec![result])
                } else {
                    emit!(MemoryEnrichmentTableReadFailed {
                        key: &key,
                        include_key_metric_tag: self.config.internal_metrics.include_key_tag
                    });
                    Ok(Default::default())
                }
            }
            Some(_) => Err(Error::OnlyEqualityConditionAllowed),
            None => Err(Error::MissingCondition { kind: "Key" }),
        }
    }

    fn add_index(&mut self, _case: Case, fields: &[&str]) -> Result<IndexHandle, Error> {
        match fields.len() {
            0 => Err(Error::MissingRequiredField { field: "Key" }),
            1 => Ok(IndexHandle(0)),
            _ => Err(Error::OnlyOneFieldAllowed),
        }
    }

    /// Returns a list of the field names that are in each index
    fn index_fields(&self) -> Vec<(Case, Vec<String>)> {
        Vec::new()
    }

    /// Doesn't need reload, data is written directly
    fn needs_reload(&self) -> bool {
        false
    }

    fn extract_state(&self) -> Option<Box<dyn std::any::Any + Send + Sync>> {
        Some(Box::new(self.clone()))
    }
}

impl std::fmt::Debug for CuckooMemoryTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CuckooMemoryTable {:?}", self.config)
    }
}

#[async_trait]
impl StreamSink<Event> for CuckooMemoryTable {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        let events_sent = register!(EventsSent::from(Output(None)));
        let bytes_sent = register!(BytesSent::from(Protocol("memory_enrichment_table".into(),)));
        let now = Instant::now();
        let scan_interval_duration = Duration::from_secs(self.config.scan_interval.into());
        let mut scan_interval = IntervalStream::new(interval_at(
            now.checked_add(scan_interval_duration).unwrap_or(now),
            scan_interval_duration,
        ));
        let mut flush_interval: Pin<Box<dyn Stream<Item = Instant> + Send>> = self
            .config
            .flush_interval
            .map(NonZeroU64::get)
            .map(Duration::from_secs)
            .map::<Pin<Box<dyn Stream<Item = Instant> + Send>>, _>(|d| {
                Box::pin(IntervalStream::new(interval(d)))
            })
            .unwrap_or(Box::pin(stream::empty()));
        let mut export_interval: Pin<Box<dyn Stream<Item = Instant> + Send>> = self
            .cuckoo_config
            .export_interval
            .map(NonZeroU64::get)
            .map(Duration::from_secs)
            .map::<Pin<Box<dyn Stream<Item = Instant> + Send>>, _>(|d| {
                Box::pin(IntervalStream::new(interval(d)))
            })
            .unwrap_or(Box::pin(stream::empty()));

        let scans_in_progress = Arc::new(AtomicUsize::new(0));
        let mut export_handle: Option<JoinHandle<()>> = None;

        loop {
            tokio::select! {
                event = input.next() => {
                    let mut event = if let Some(event) = event {
                        event
                    } else {
                        break;
                    };
                    let event_byte_size = event.estimated_json_encoded_size_of();

                    let finalizers = event.take_finalizers();

                    // Panic: This sink only accepts Logs, so this should never panic
                    let log = event.into_log();

                    if let (Value::Object(map), _) = log.into_parts() {
                        self.handle_value(map)
                    };

                    finalizers.update_status(EventStatus::Delivered);
                    events_sent.emit(CountByteSize(1, event_byte_size));
                    bytes_sent.emit(ByteSize(event_byte_size.get()));

                    if self.config.flush_interval.is_none() {
                        emit!(MemoryEnrichmentTableFlushed {
                            new_objects_count: self.filter.get_item_count(),
                            new_byte_size: self.filter.get_memory_usage()
                        });
                    }
                },

                Some(_) = flush_interval.next() => {
                    emit!(MemoryEnrichmentTableFlushed {
                        new_objects_count: self.filter.get_item_count(),
                        new_byte_size: self.filter.get_memory_usage()
                    });
                }

                Some(_) = export_interval.next() => {
                    if export_handle.as_ref().is_some_and(|h| !h.is_finished()) {

                        warn!("Previous export still in progress for cuckoo enrichment table. New export will be skipped until previous one is complete. Consider increasing export interval.");
                        continue;
                    } else if let Some(handle) = export_handle
                        && let Err(join_error) = handle.join() {
                        warn!("Cuckoo enrichment table export failed: {:?}", join_error);
                    }
                    let exporting_instance = self.clone();
                    export_handle = Some(std::thread::spawn(move || {
                        exporting_instance.export();
                    }));
                }

                Some(_) = scan_interval.next() => {
                    if scans_in_progress.load(Ordering::Acquire) > 0 {
                        warn!("Previous scan still in progress for cuckoo enrichment table. New scan will be skipped until previous one is complete. Consider increasing scan interval.");
                        continue;
                    }
                    self.scan(&scans_in_progress).await;
                }
            }
        }

        if let Some(handle) = export_handle
            && let Err(join_error) = handle.join()
        {
            warn!("Cuckoo enrichment table export failed: {:?}", join_error);
        }

        // Final export before exiting
        self.export();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZero;

    use futures::future::ready;
    use vector_lib::{event::LogEvent, sink::VectorSink};

    use crate::test_util::components::{SINK_TAGS, run_and_assert_sink_compliance};

    use super::*;

    fn build_cuckoo_config(modfn: impl Fn(&mut CuckooMemoryConfig)) -> CuckooMemoryConfig {
        let mut config = CuckooMemoryConfig {
            fingerprint_bits: default_cuckoo_fingerprint_bits(),
            bucket_size: default_cuckoo_bucket_size(),
            max_entries: 1000,
            max_kicks: default_cuckoo_max_kicks(),
            lru_enabled: false,
            lru_deletion_enabled: false,
            ttl_enabled: false,
            ttl_bits: default_cuckoo_ttl_bits(),
            counter_enabled: false,
            counter_bits: default_cuckoo_counter_bits(),
            counter_field: OptionalValuePath::none(),
            counter_insertion_increment: default_cuckoo_counter_insertion_increment(),
            counter_lookup_increment: default_cuckoo_counter_lookup_increment(),
            persistence_path: None,
            export_interval: None,
            scanning_threads: None,
            concurrent_scanning: false,
            lru_bits: default_cuckoo_lru_bits(),
            lru_starting_value: default_cuckoo_lru_starting_value(),
            lru_increment: default_cuckoo_lru_increment(),
            lru_aging_strategy: CuckooLruAgingStrategy::default(),
        };
        modfn(&mut config);
        config
    }

    #[test]
    fn finds_row() {
        let memory = CuckooMemoryTable::new(Default::default(), build_cuckoo_config(|_| {}))
            .expect("default cuckoo memory table should build correctly");
        memory.handle_value(ObjectMap::from([("test_key".into(), Value::from(5))]));

        let condition = Condition::Equals {
            field: "key",
            value: Value::from("test_key"),
        };

        let result = memory.find_table_row(Case::Sensitive, &[condition], None, None, None);
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.get("key").unwrap(), &Value::from("test_key"));
        // Cuckoo fingerprint is provided too
        assert!(result.contains_key("fingerprint"));
    }

    #[tokio::test]
    async fn sink_spec_compliance() {
        let event = Event::Log(LogEvent::from(ObjectMap::from([(
            "test_key".into(),
            Value::from(5),
        )])));

        let memory = CuckooMemoryTable::new(Default::default(), build_cuckoo_config(|_| {}))
            .expect("default cuckoo memory table should build correctly");

        run_and_assert_sink_compliance(
            VectorSink::from_event_streamsink(memory),
            stream::once(ready(event)),
            &SINK_TAGS,
        )
        .await;
    }

    #[test]
    fn missing_key() {
        let memory = CuckooMemoryTable::new(Default::default(), build_cuckoo_config(|_| {}))
            .expect("default cuckoo memory table should build correctly");

        let condition = Condition::Equals {
            field: "key",
            value: Value::from("test_key"),
        };

        assert!(
            memory
                .find_table_rows(Case::Sensitive, &[condition], None, None, None)
                .unwrap()
                .pop()
                .is_none()
        );
    }

    #[tokio::test]
    async fn updates_ttl_on_scan_interval() {
        let ttl = 100;
        let mut core_conf = MemoryConfig::default();
        core_conf.ttl = ttl;
        core_conf.scan_interval = NonZero::new(1).unwrap();
        let memory = CuckooMemoryTable::new(
            core_conf,
            build_cuckoo_config(|c| {
                c.ttl_enabled = true;
                c.ttl_bits = NonZero::new(8).unwrap();
            }),
        )
        .expect("TTL cuckoo memory table should build correctly");

        memory.handle_value(ObjectMap::from([("test_key".into(), Value::from(5))]));

        let condition = Condition::Equals {
            field: "key",
            value: Value::from("test_key"),
        };

        let result = memory.find_table_row(
            Case::Sensitive,
            std::slice::from_ref(&condition),
            None,
            None,
            None,
        );
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.get("key").unwrap(), &Value::from("test_key"));
        assert_eq!(result.get("ttl").unwrap(), &Value::from(100));

        memory.scan(&Arc::new(AtomicUsize::default())).await;

        let result = memory.find_table_row(Case::Sensitive, &[condition], None, None, None);
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.get("key").unwrap(), &Value::from("test_key"));
        assert_eq!(result.get("ttl").unwrap(), &Value::from(99));
    }

    #[test]
    fn restores_state() {
        let memory = CuckooMemoryTable::new(Default::default(), build_cuckoo_config(|_| {}))
            .expect("default cuckoo memory table should build correctly");
        memory.handle_value(ObjectMap::from([("test_key".into(), Value::from(5))]));

        let condition = Condition::Equals {
            field: "key",
            value: Value::from("test_key"),
        };

        let result = memory.find_table_row(
            Case::Sensitive,
            std::slice::from_ref(&condition),
            None,
            None,
            None,
        );
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.get("key").unwrap(), &Value::from("test_key"));
        // Cuckoo fingerprint is provided too
        assert!(result.contains_key("fingerprint"));

        let restored_memory = CuckooMemoryTable::from_previous_state(
            Default::default(),
            build_cuckoo_config(|_| {}),
            memory
                .extract_state()
                .expect("cuckoo memory table should allow state extraction"),
        )
        .expect("cuckoo memory table build from previous state should succeed");

        let result =
            restored_memory.find_table_row(Case::Sensitive, &[condition], None, None, None);
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.get("key").unwrap(), &Value::from("test_key"));
        // Cuckoo fingerprint is provided too
        assert!(result.contains_key("fingerprint"));
    }
}
