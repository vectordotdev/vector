use std::{
    sync::{Arc, RwLock},
    time::Duration,
};

use async_trait::async_trait;
use bloomy::BloomFilter;
use bytes::Bytes;
use futures::{StreamExt, stream::BoxStream};
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
    sink::StreamSink,
};
use vrl::value::{KeyString, ObjectMap, Value};

use crate::enrichment_tables::memory::{
    MemoryConfig,
    internal_events::{
        MemoryEnrichmentTableFlushed, MemoryEnrichmentTableInserted, MemoryEnrichmentTableRead,
        MemoryEnrichmentTableReadFailed,
    },
};

/// A struct that implements [vector_lib::enrichment::Table] to handle loading enrichment data from a bloom table.
#[derive(Clone)]
pub(super) struct BloomMemoryTable {
    filter: Arc<RwLock<BloomFilter<String>>>,
    pub(super) config: MemoryConfig,
}

/// Configuration of bloom filter for memory table.
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "type")]
pub struct BloomMemoryConfig {
    /// Maximum number of entries that can be stored in the filter
    pub max_entries: usize,
}

impl BloomMemoryTable {
    /// Creates a new [BloomMemoryTable] based on the provided config.
    pub(super) fn new(
        config: MemoryConfig,
        bloom_config: BloomMemoryConfig,
    ) -> crate::Result<Self> {
        let filter = Arc::new(RwLock::new(BloomFilter::new(bloom_config.max_entries)));

        Ok(Self { config, filter })
    }

    fn handle_value(&self, value: ObjectMap) {
        for (k, _) in value.iter() {
            self.filter
                .write()
                .expect("rwlock poisoned")
                .insert(&k.to_string());
            emit!(MemoryEnrichmentTableInserted {
                key: k,
                include_key_metric_tag: self.config.internal_metrics.include_key_tag
            });
        }
    }
}

impl Table for BloomMemoryTable {
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
                let key = value.to_string_lossy().to_string();
                if self.filter.read().expect("rwlock poisoned").contains(&key) {
                    emit!(MemoryEnrichmentTableRead {
                        key: &key,
                        include_key_metric_tag: self.config.internal_metrics.include_key_tag
                    });
                    let result = ObjectMap::from([(
                        KeyString::from("key"),
                        Value::Bytes(Bytes::copy_from_slice(key.as_bytes())),
                    )]);
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

impl std::fmt::Debug for BloomMemoryTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BloomMemoryTable {:?}", self.config)
    }
}

#[async_trait]
impl StreamSink<Event> for BloomMemoryTable {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        let events_sent = register!(EventsSent::from(Output(None)));
        let bytes_sent = register!(BytesSent::from(Protocol("memory_enrichment_table".into(),)));
        let mut flush_interval = IntervalStream::new(interval(
            self.config
                .flush_interval
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
                    let filter = self.filter.read().expect("rwlock poisoned");
                    emit!(MemoryEnrichmentTableFlushed {
                        new_objects_count: filter.count(),
                        new_byte_size: filter.bits() / 8
                    });
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_bloom_config(modfn: impl Fn(&mut BloomMemoryConfig)) -> BloomMemoryConfig {
        let mut config = BloomMemoryConfig { max_entries: 1000 };
        modfn(&mut config);
        config
    }

    #[test]
    fn finds_row() {
        let memory = BloomMemoryTable::new(Default::default(), build_bloom_config(|_| {}))
            .expect("default bloom memory table should build correctly");
        memory.handle_value(ObjectMap::from([("test_key".into(), Value::from(5))]));

        let condition = Condition::Equals {
            field: "key",
            value: Value::from("test_key"),
        };

        let result = memory.find_table_row(Case::Sensitive, &[condition], None, None, None);
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.get("key").unwrap(), &Value::from("test_key"));
    }
}
