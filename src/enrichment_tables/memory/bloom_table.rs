use std::{
    num::{NonZeroU64, NonZeroUsize},
    pin::Pin,
    sync::{Arc, RwLock},
    time::Duration,
};

use async_trait::async_trait;
use bloomy::{BloomFilter, bloom};
use bytes::Bytes;
use futures::{
    Stream, StreamExt,
    stream::{self, BoxStream},
};
use tokio::time::{Instant, interval};
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
    bloom_config: BloomMemoryConfig,
}

/// Configuration of bloom filter for memory table.
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BloomMemoryConfig {
    /// Maximum number of entries that can be stored in the filter
    pub max_entries: NonZeroUsize,
}

impl BloomMemoryConfig {
    /// Returns the size of the filter in bytes for this configuration.
    pub(super) fn filter_size(&self) -> u64 {
        bloom::optimal_bits(self.max_entries.get(), bloom::DEFAULT_FALSE_POSITIVE_RATE).div_ceil(8)
            as u64
    }
}

impl BloomMemoryTable {
    /// Creates a new [BloomMemoryTable] based on the provided config.
    pub(super) fn new(
        config: MemoryConfig,
        bloom_config: BloomMemoryConfig,
    ) -> crate::Result<Self> {
        let filter_size = bloom_config.filter_size();
        if let Some(max_byte_size) = config.max_byte_size
            && filter_size > max_byte_size
        {
            return Err(format!("Configured bloom filter is larger ({}) than defined `max_byte_size` ({}). Reduce the size of bloom filter or increase or remove `max_byte_size`.", filter_size, max_byte_size).into());
        }
        let filter = Arc::new(RwLock::new(BloomFilter::new(
            bloom_config.max_entries.get(),
        )));

        Ok(Self {
            config,
            filter,
            bloom_config,
        })
    }

    /// Creates a new [BloomMemoryTable] based on the provided config and previous state.
    pub(super) fn from_previous_state(
        config: MemoryConfig,
        bloom_config: BloomMemoryConfig,
        prev_state: Box<dyn std::any::Any + Send + Sync>,
    ) -> crate::Result<Self> {
        if let Ok(prev_memory) = prev_state.downcast::<BloomMemoryTable>() {
            if prev_memory.bloom_config == bloom_config {
                let filter_size = bloom_config.filter_size();
                if let Some(max_byte_size) = config.max_byte_size
                    && filter_size > max_byte_size
                {
                    return Err(format!("Configured bloom filter is larger ({}) than defined `max_byte_size` ({}). Reduce the size of bloom filter or increase or remove `max_byte_size`.", filter_size, max_byte_size).into());
                }
                Ok(Self {
                    filter: prev_memory.filter,
                    config,
                    bloom_config,
                })
            } else {
                Self::new(config, bloom_config)
            }
        } else {
            Self::new(config, bloom_config)
        }
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
                    let result = ObjectMap::from([
                        (
                            KeyString::from("key"),
                            Value::Bytes(Bytes::copy_from_slice(key.as_bytes())),
                        ),
                        (KeyString::from("value"), Value::Null),
                    ]);
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
        let mut flush_interval: Pin<Box<dyn Stream<Item = Instant> + Send>> = self
            .config
            .flush_interval
            .map(NonZeroU64::get)
            .map(Duration::from_secs)
            .map::<Pin<Box<dyn Stream<Item = Instant> + Send>>, _>(|d| {
                Box::pin(IntervalStream::new(interval(d)))
            })
            .unwrap_or(Box::pin(stream::empty()));

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
    use futures::future::ready;
    use vector_lib::{event::LogEvent, sink::VectorSink};

    use crate::test_util::components::{SINK_TAGS, run_and_assert_sink_compliance};

    use super::*;

    fn build_bloom_config(modfn: impl Fn(&mut BloomMemoryConfig)) -> BloomMemoryConfig {
        let mut config = BloomMemoryConfig {
            max_entries: NonZeroUsize::new(1000).unwrap(),
        };
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

    #[tokio::test]
    async fn sink_spec_compliance() {
        let event = Event::Log(LogEvent::from(ObjectMap::from([(
            "test_key".into(),
            Value::from(5),
        )])));

        let memory = BloomMemoryTable::new(Default::default(), build_bloom_config(|_| {}))
            .expect("default bloom memory table should build correctly");

        run_and_assert_sink_compliance(
            VectorSink::from_event_streamsink(memory),
            stream::once(ready(event)),
            &SINK_TAGS,
        )
        .await;
    }

    #[test]
    fn missing_key() {
        let memory = BloomMemoryTable::new(Default::default(), build_bloom_config(|_| {}))
            .expect("default bloom memory table should build correctly");

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

    #[test]
    fn restores_state() {
        let memory = BloomMemoryTable::new(Default::default(), build_bloom_config(|_| {}))
            .expect("default bloom memory table should build correctly");
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

        let restored_memory = BloomMemoryTable::from_previous_state(
            Default::default(),
            build_bloom_config(|_| {}),
            memory
                .extract_state()
                .expect("bloom memory table should allow state extraction"),
        )
        .expect("bloom memory table build from previous state should succeed");

        let result =
            restored_memory.find_table_row(Case::Sensitive, &[condition], None, None, None);
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.get("key").unwrap(), &Value::from("test_key"));
    }
}
