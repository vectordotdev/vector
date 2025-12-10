use std::{
    fs::File,
    io::{BufReader, BufWriter, Write},
    num::NonZeroUsize,
    path::PathBuf,
    time::Duration,
};

use async_trait::async_trait;
use bytes::Bytes;
use cuckoo_clock::{
    CuckooFilter, ExportableRandomState, InsertValues, LookupValues,
    config::{CounterConfig, CuckooConfiguration, LruConfig, TtlConfig},
};
use futures::{StreamExt, stream::BoxStream};
use tempfile::NamedTempFile;
use tokio::time::interval;
use tokio_stream::wrappers::IntervalStream;
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
        MemoryEnrichmentTableFlushed, MemoryEnrichmentTableInserted, MemoryEnrichmentTableRead,
        MemoryEnrichmentTableReadFailed, MemoryEnrichmentTableRemoved,
        MemoryEnrichmentTableTtlExpiredCount,
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
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "type")]
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
    /// Field in the incoming value used as the counter override.
    #[configurable(derived)]
    #[serde(default)]
    pub counter_field: OptionalValuePath,
    /// Path to the file to export data to periodically and on exit.
    /// Data will be imported from this file on startup.
    #[configurable(derived)]
    #[serde(default)]
    pub persistence_path: Option<PathBuf>,
    /// The interval used for exporting data.
    ///
    /// By default, export is only done on exit.
    #[serde(skip_serializing_if = "vector_lib::serde::is_default")]
    pub export_interval: Option<u64>,
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

const fn default_cuckoo_counter_bits() -> NonZeroUsize {
    unsafe { NonZeroUsize::new_unchecked(8) }
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
        let ttl_val = config.ttl / config.scan_interval.get();
        let mut builder = CuckooConfiguration::builder(cuckoo_config.max_entries)
            .fingerprint_bits(cuckoo_config.fingerprint_bits.get().try_into()?)
            .bucket_size(cuckoo_config.bucket_size)
            .max_kicks(cuckoo_config.max_kicks);

        if cuckoo_config.lru_enabled {
            builder = builder.with_lru(LruConfig::default());
        }

        if cuckoo_config.ttl_enabled {
            builder = builder.with_ttl(TtlConfig {
                ttl: u32::try_from(ttl_val)?.try_into()?,
                ttl_bits: cuckoo_config.ttl_bits.get().try_into()?,
            });
        }

        if cuckoo_config.counter_enabled {
            builder = builder.with_counter(CounterConfig {
                counter_bits: cuckoo_config.counter_bits.get().try_into()?,
                ..Default::default()
            });
        }

        let built_config = builder.build()?;

        let filter = 'import: {
            if let Some(path) = &cuckoo_config.persistence_path {
                let Ok(file) = File::open(path) else {
                    warn!(
                        "Couldn't open \"{}\" for cuckoo filter state import.",
                        path.to_str().unwrap_or("")
                    );
                    break 'import CuckooFilter::new_random_exportable(built_config);
                };
                let mut reader = BufReader::new(file);
                let filter = match CuckooFilter::import_random_exportable(&mut reader) {
                    Ok(filter) => filter,
                    Err(error) => {
                        warn!("Cuckoo filter state import failed: {}", error);
                        break 'import CuckooFilter::new_random_exportable(built_config);
                    }
                };

                if filter.get_configuration() != built_config {
                    // TODO: Should this stop the build from succeeding? The import will be lost,
                    // because it will be overwritter very soon.
                    warn!(
                        "Stored cuckoo filter configuration doesn't match with new configuration. Ignoring the import.",
                    );
                    break 'import CuckooFilter::new_random_exportable(built_config);
                }

                filter
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

    fn export(&self) {
        if let Some(path) = &self.cuckoo_config.persistence_path {
            let mut parent = path.clone();
            if parent.pop()
                && let Ok(temp) = NamedTempFile::new_in(parent)
            {
                {
                    let mut writer = BufWriter::new(temp.as_file());
                    if self.export_to(&mut writer).is_err() {
                        return;
                    }
                }
                if let Err(error) = temp.persist(path) {
                    warn!("Cuckoo filter export failed: {}", error);
                }
            } else {
                warn!(
                    "Couldn't open temporary file for export. Trying to write directly to \"{}\"",
                    path.to_str().unwrap_or("")
                );
                let Ok(file) = File::create(path) else {
                    warn!(
                        "Couldn't open \"{}\" for cuckoo filter state export.",
                        path.to_str().unwrap_or("")
                    );
                    return;
                };
                let mut writer = BufWriter::new(file);
                let _ = self.export_to(&mut writer);
            };
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

            if self.cuckoo_config.ttl_enabled || self.cuckoo_config.counter_enabled {
                let ttl = self
                    .config
                    .ttl_field
                    .path
                    .as_ref()
                    .and_then(|p| value.get(p))
                    .and_then(|v| v.as_integer())
                    .and_then(|v| u64::try_from(v).ok())
                    .map(|v| v / self.config.scan_interval.get())
                    .and_then(|v| u32::try_from(v).ok());
                let counter = self
                    .cuckoo_config
                    .counter_field
                    .path
                    .as_ref()
                    .and_then(|p| value.get(p))
                    .and_then(|v| v.as_integer())
                    .and_then(|v| i32::try_from(v).ok());
                let _ = self.filter.insert_if_not_present_with_update(
                    k,
                    InsertValues { ttl, counter },
                    LookupValues {
                        ttl,
                        counter_diff: counter,
                    },
                );
            } else {
                let _ = self.filter.insert_if_not_present(k);
            }
            emit!(MemoryEnrichmentTableInserted {
                key: k,
                include_key_metric_tag: self.config.internal_metrics.include_key_tag
            });
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
        let mut scan_interval = IntervalStream::new(interval(Duration::from_secs(
            self.config.scan_interval.into(),
        )));
        let mut flush_interval = IntervalStream::new(interval(
            self.config
                .flush_interval
                .map(Duration::from_secs)
                .unwrap_or(Duration::MAX),
        ));
        let mut export_interval = IntervalStream::new(interval(
            self.cuckoo_config
                .export_interval
                .map(Duration::from_secs)
                .unwrap_or(Duration::MAX),
        ));

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
                },

                Some(_) = flush_interval.next() => {
                    emit!(MemoryEnrichmentTableFlushed {
                        new_objects_count: self.filter.get_item_count(),
                        new_byte_size: self.filter.get_memory_usage()
                    });
                }

                Some(_) = export_interval.next() => {
                    self.export();
                }

                Some(_) = scan_interval.next() => {
                    let expired = self.filter.scan_and_update_full();
                    emit!(MemoryEnrichmentTableTtlExpiredCount {
                        count: expired as u64
                    });
                }
            }
        }

        // Final export before exiting
        self.export();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // fn build_memory_config(modfn: impl Fn(&mut MemoryConfig)) -> MemoryConfig {
    //     let mut config = MemoryConfig::default();
    //     modfn(&mut config);
    //     config
    // }

    fn build_cuckoo_config(modfn: impl Fn(&mut CuckooMemoryConfig)) -> CuckooMemoryConfig {
        let mut config = CuckooMemoryConfig {
            fingerprint_bits: default_cuckoo_fingerprint_bits(),
            bucket_size: default_cuckoo_bucket_size(),
            max_entries: 1000,
            max_kicks: default_cuckoo_max_kicks(),
            lru_enabled: false,
            ttl_enabled: false,
            ttl_bits: default_cuckoo_ttl_bits(),
            counter_enabled: false,
            counter_bits: default_cuckoo_counter_bits(),
            counter_field: OptionalValuePath::none(),
            persistence_path: None,
            export_interval: None,
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
}
