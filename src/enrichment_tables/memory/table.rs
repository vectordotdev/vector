use crate::enrichment_tables::memory::internal_events::{
    MemoryEnrichmentTableFlushed, MemoryEnrichmentTableInsertFailed, MemoryEnrichmentTableInserted,
    MemoryEnrichmentTableRead, MemoryEnrichmentTableReadFailed, MemoryEnrichmentTableTtlExpired,
};
use crate::enrichment_tables::memory::MemoryConfig;
use crate::SourceSender;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use evmap::shallow_copy::CopyValue;
use evmap::{self};
use evmap_derive::ShallowCopy;
use futures::StreamExt;
use thread_local::ThreadLocal;
use tokio::time::interval;
use tokio_stream::wrappers::IntervalStream;
use vector_lib::config::LogNamespace;
use vector_lib::shutdown::ShutdownSignal;
use vector_lib::{ByteSizeOf, EstimatedJsonEncodedSizeOf};

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::BoxStream;
use vector_lib::enrichment::{Case, Condition, IndexHandle, Table};
use vector_lib::event::{Event, EventStatus, Finalizable};
use vector_lib::internal_event::{
    ByteSize, BytesSent, CountByteSize, EventsSent, InternalEventHandle, Output, Protocol,
};
use vector_lib::sink::StreamSink;
use vrl::value::{KeyString, ObjectMap, Value};

use super::source::MemorySource;

/// Single memory entry containing the value and TTL
#[derive(Clone, Eq, PartialEq, Hash, ShallowCopy)]
pub struct MemoryEntry {
    value: String,
    update_time: CopyValue<Instant>,
}

impl ByteSizeOf for MemoryEntry {
    fn allocated_bytes(&self) -> usize {
        self.value.size_of()
    }
}

impl MemoryEntry {
    pub(super) fn as_object_map(
        &self,
        now: Instant,
        total_ttl: u64,
        key: &str,
    ) -> Result<ObjectMap, String> {
        let ttl = total_ttl.saturating_sub(now.duration_since(*self.update_time).as_secs());
        Ok(ObjectMap::from([
            (
                KeyString::from("key"),
                Value::Bytes(Bytes::copy_from_slice(key.as_bytes())),
            ),
            (
                KeyString::from("value"),
                serde_json::from_str::<Value>(&self.value)
                    .map_err(|_| "Failed to read value from memory!")?,
            ),
            (
                KeyString::from("ttl"),
                Value::Integer(ttl.try_into().unwrap_or(i64::MAX)),
            ),
        ]))
    }

    fn expired(&self, now: Instant, ttl: u64) -> bool {
        now.duration_since(*self.update_time).as_secs() > ttl
    }
}

#[derive(Default)]
struct MemoryMetadata {
    byte_size: u64,
}

// Used to ensure that these 2 are locked together
pub(super) struct MemoryWriter {
    pub(super) write_handle: evmap::WriteHandle<String, MemoryEntry>,
    metadata: MemoryMetadata,
}

/// A struct that implements [vector_lib::enrichment::Table] to handle loading enrichment data from a memory structure.
pub struct Memory {
    pub(super) read_handle_factory: evmap::ReadHandleFactory<String, MemoryEntry>,
    pub(super) read_handle: ThreadLocal<evmap::ReadHandle<String, MemoryEntry>>,
    pub(super) write_handle: Arc<Mutex<MemoryWriter>>,
    pub(super) config: MemoryConfig,
}

impl Memory {
    /// Creates a new [Memory] based on the provided config.
    pub fn new(config: MemoryConfig) -> Self {
        let (read_handle, write_handle) = evmap::new();
        Self {
            config,
            read_handle_factory: read_handle.factory(),
            read_handle: ThreadLocal::new(),
            write_handle: Arc::new(Mutex::new(MemoryWriter {
                write_handle,
                metadata: MemoryMetadata::default(),
            })),
        }
    }

    pub(super) fn get_read_handle(&self) -> &evmap::ReadHandle<String, MemoryEntry> {
        self.read_handle
            .get_or(|| self.read_handle_factory.handle())
    }

    fn handle_value(&self, value: ObjectMap) {
        let mut writer = self.write_handle.lock().expect("mutex poisoned");
        let now = Instant::now();

        for (k, v) in value.into_iter() {
            let new_entry_key = String::from(k);
            let Ok(v) = serde_json::to_string(&v) else {
                emit!(MemoryEnrichmentTableInsertFailed {
                    key: &new_entry_key,
                    include_key_metric_tag: self.config.internal_metrics.include_key_tag
                });
                continue;
            };
            let new_entry = MemoryEntry {
                value: v,
                update_time: now.into(),
            };
            let new_entry_size = new_entry_key.size_of() + new_entry.size_of();
            if let Some(max_byte_size) = self.config.max_byte_size {
                if writer
                    .metadata
                    .byte_size
                    .saturating_add(new_entry_size as u64)
                    > max_byte_size
                {
                    // Reject new entries
                    emit!(MemoryEnrichmentTableInsertFailed {
                        key: &new_entry_key,
                        include_key_metric_tag: self.config.internal_metrics.include_key_tag
                    });
                    continue;
                }
            }
            writer.metadata.byte_size = writer
                .metadata
                .byte_size
                .saturating_add(new_entry_size as u64);
            emit!(MemoryEnrichmentTableInserted {
                key: &new_entry_key,
                include_key_metric_tag: self.config.internal_metrics.include_key_tag
            });
            writer.write_handle.update(new_entry_key, new_entry);
        }

        if self.config.flush_interval.is_none() {
            self.flush(writer);
        }
    }

    fn scan_and_mark_for_deletion(&self, writer: &mut MutexGuard<'_, MemoryWriter>) -> bool {
        let now = Instant::now();

        let mut needs_flush = false;
        // Since evmap holds 2 separate maps for the data, we are free to directly remove
        // elements via the writer, while we are iterating the reader
        // Refresh will happen only after we manually invoke it after iteration
        if let Some(reader) = self.get_read_handle().read() {
            for (k, v) in reader.iter() {
                if let Some(entry) = v.get_one() {
                    if entry.expired(now, self.config.ttl) {
                        // Byte size is not reduced at this point, because the actual deletion
                        // will only happen at refresh time
                        writer.write_handle.empty(k.clone());
                        emit!(MemoryEnrichmentTableTtlExpired {
                            key: k,
                            include_key_metric_tag: self.config.internal_metrics.include_key_tag
                        });
                        needs_flush = true;
                    }
                }
            }
        };

        needs_flush
    }

    fn scan(&self, mut writer: MutexGuard<'_, MemoryWriter>) {
        let needs_flush = self.scan_and_mark_for_deletion(&mut writer);
        if needs_flush {
            self.flush(writer);
        }
    }

    fn flush(&self, mut writer: MutexGuard<'_, MemoryWriter>) {
        writer.write_handle.refresh();
        if let Some(reader) = self.get_read_handle().read() {
            let mut byte_size = 0;
            for (k, v) in reader.iter() {
                byte_size += k.size_of() + v.get_one().size_of();
            }
            writer.metadata.byte_size = byte_size as u64;
            emit!(MemoryEnrichmentTableFlushed {
                new_objects_count: reader.len(),
                new_byte_size: byte_size
            });
        }
    }

    pub(crate) fn as_source(
        &self,
        shutdown: ShutdownSignal,
        out: SourceSender,
        log_namespace: LogNamespace,
    ) -> MemorySource {
        MemorySource {
            memory: self.clone(),
            shutdown,
            out,
            log_namespace,
        }
    }
}

impl Clone for Memory {
    fn clone(&self) -> Self {
        Self {
            read_handle_factory: self.read_handle_factory.clone(),
            read_handle: ThreadLocal::new(),
            write_handle: Arc::clone(&self.write_handle),
            config: self.config.clone(),
        }
    }
}

impl Table for Memory {
    fn find_table_row<'a>(
        &self,
        case: Case,
        condition: &'a [Condition<'a>],
        select: Option<&'a [String]>,
        wildcard: Option<&Value>,
        index: Option<IndexHandle>,
    ) -> Result<ObjectMap, String> {
        let mut rows = self.find_table_rows(case, condition, select, wildcard, index)?;

        match rows.pop() {
            Some(row) if rows.is_empty() => Ok(row),
            Some(_) => Err("More than 1 row found".to_string()),
            None => Err("Key not found".to_string()),
        }
    }

    fn find_table_rows<'a>(
        &self,
        _case: Case,
        condition: &'a [Condition<'a>],
        _select: Option<&'a [String]>,
        _wildcard: Option<&Value>,
        _index: Option<IndexHandle>,
    ) -> Result<Vec<ObjectMap>, String> {
        match condition.first() {
            Some(_) if condition.len() > 1 => Err("Only one condition is allowed".to_string()),
            Some(Condition::Equals { value, .. }) => {
                let key = value.to_string_lossy();
                match self.get_read_handle().get_one(key.as_ref()) {
                    Some(row) => {
                        emit!(MemoryEnrichmentTableRead {
                            key: &key,
                            include_key_metric_tag: self.config.internal_metrics.include_key_tag
                        });
                        row.as_object_map(Instant::now(), self.config.ttl, &key)
                            .map(|r| vec![r])
                    }
                    None => {
                        emit!(MemoryEnrichmentTableReadFailed {
                            key: &key,
                            include_key_metric_tag: self.config.internal_metrics.include_key_tag
                        });
                        Ok(Default::default())
                    }
                }
            }
            Some(_) => Err("Only equality condition is allowed".to_string()),
            None => Err("Key condition must be specified".to_string()),
        }
    }

    fn add_index(&mut self, _case: Case, fields: &[&str]) -> Result<IndexHandle, String> {
        match fields.len() {
            0 => Err("Key field is required".to_string()),
            1 => Ok(IndexHandle(0)),
            _ => Err("Only one field is allowed".to_string()),
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

impl std::fmt::Debug for Memory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Memory {} row(s)", self.get_read_handle().len())
    }
}

#[async_trait]
impl StreamSink<Event> for Memory {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        let events_sent = register!(EventsSent::from(Output(None)));
        let bytes_sent = register!(BytesSent::from(Protocol("memory_enrichment_table".into(),)));
        let mut flush_interval = IntervalStream::new(interval(
            self.config
                .flush_interval
                .map(Duration::from_secs)
                .unwrap_or(Duration::MAX),
        ));
        let mut scan_interval = IntervalStream::new(interval(Duration::from_secs(
            self.config.scan_interval.into(),
        )));

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
                }

                Some(_) = flush_interval.next() => {
                    let writer = self.write_handle.lock().expect("mutex poisoned");
                    self.flush(writer);
                }

                Some(_) = scan_interval.next() => {
                    let writer = self.write_handle.lock().expect("mutex poisoned");
                    self.scan(writer);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use futures::{future::ready, StreamExt};
    use futures_util::stream;
    use std::{num::NonZeroU64, time::Duration};
    use tokio::time;

    use vector_lib::{
        event::{EventContainer, MetricValue},
        metrics::Controller,
        sink::VectorSink,
    };

    use super::*;
    use crate::{
        enrichment_tables::memory::{
            internal_events::InternalMetricsConfig, source::MemorySourceConfig,
        },
        event::{Event, LogEvent},
        test_util::components::{
            run_and_assert_sink_compliance, run_and_assert_source_compliance, SINK_TAGS,
            SOURCE_TAGS,
        },
    };

    fn build_memory_config(modfn: impl Fn(&mut MemoryConfig)) -> MemoryConfig {
        let mut config = MemoryConfig::default();
        modfn(&mut config);
        config
    }

    #[test]
    fn finds_row() {
        let memory = Memory::new(Default::default());
        memory.handle_value(ObjectMap::from([("test_key".into(), Value::from(5))]));

        let condition = Condition::Equals {
            field: "key",
            value: Value::from("test_key"),
        };

        assert_eq!(
            Ok(ObjectMap::from([
                ("key".into(), Value::from("test_key")),
                ("ttl".into(), Value::from(memory.config.ttl)),
                ("value".into(), Value::from(5)),
            ])),
            memory.find_table_row(Case::Sensitive, &[condition], None, None, None)
        );
    }

    #[test]
    fn calculates_ttl() {
        let ttl = 100;
        let secs_to_subtract = 10;
        let memory = Memory::new(build_memory_config(|c| c.ttl = ttl));
        {
            let mut handle = memory.write_handle.lock().unwrap();
            handle.write_handle.update(
                "test_key".to_string(),
                MemoryEntry {
                    value: "5".to_string(),
                    update_time: (Instant::now() - Duration::from_secs(secs_to_subtract)).into(),
                },
            );
            handle.write_handle.refresh();
        }

        let condition = Condition::Equals {
            field: "key",
            value: Value::from("test_key"),
        };

        assert_eq!(
            Ok(ObjectMap::from([
                ("key".into(), Value::from("test_key")),
                ("ttl".into(), Value::from(ttl - secs_to_subtract)),
                ("value".into(), Value::from(5)),
            ])),
            memory.find_table_row(Case::Sensitive, &[condition], None, None, None)
        );
    }

    #[test]
    fn removes_expired_records_on_scan_interval() {
        let ttl = 100;
        let memory = Memory::new(build_memory_config(|c| {
            c.ttl = ttl;
        }));
        {
            let mut handle = memory.write_handle.lock().unwrap();
            handle.write_handle.update(
                "test_key".to_string(),
                MemoryEntry {
                    value: "5".to_string(),
                    update_time: (Instant::now() - Duration::from_secs(ttl + 10)).into(),
                },
            );
            handle.write_handle.refresh();
        }

        // Finds the value before scan
        let condition = Condition::Equals {
            field: "key",
            value: Value::from("test_key"),
        };
        assert_eq!(
            Ok(ObjectMap::from([
                ("key".into(), Value::from("test_key")),
                ("ttl".into(), Value::from(0)),
                ("value".into(), Value::from(5)),
            ])),
            memory.find_table_row(Case::Sensitive, &[condition.clone()], None, None, None)
        );

        // Force scan
        let writer = memory.write_handle.lock().unwrap();
        memory.scan(writer);

        // The value is not present anymore
        assert!(memory
            .find_table_rows(Case::Sensitive, &[condition], None, None, None)
            .unwrap()
            .pop()
            .is_none());
    }

    #[test]
    fn does_not_show_values_before_flush_interval() {
        let ttl = 100;
        let memory = Memory::new(build_memory_config(|c| {
            c.ttl = ttl;
            c.flush_interval = Some(10);
        }));
        memory.handle_value(ObjectMap::from([("test_key".into(), Value::from(5))]));

        let condition = Condition::Equals {
            field: "key",
            value: Value::from("test_key"),
        };

        assert!(memory
            .find_table_rows(Case::Sensitive, &[condition], None, None, None)
            .unwrap()
            .pop()
            .is_none());
    }

    #[test]
    fn updates_ttl_on_value_replacement() {
        let ttl = 100;
        let memory = Memory::new(build_memory_config(|c| c.ttl = ttl));
        {
            let mut handle = memory.write_handle.lock().unwrap();
            handle.write_handle.update(
                "test_key".to_string(),
                MemoryEntry {
                    value: "5".to_string(),
                    update_time: (Instant::now() - Duration::from_secs(ttl / 2)).into(),
                },
            );
            handle.write_handle.refresh();
        }
        let condition = Condition::Equals {
            field: "key",
            value: Value::from("test_key"),
        };

        assert_eq!(
            Ok(ObjectMap::from([
                ("key".into(), Value::from("test_key")),
                ("ttl".into(), Value::from(ttl / 2)),
                ("value".into(), Value::from(5)),
            ])),
            memory.find_table_row(Case::Sensitive, &[condition.clone()], None, None, None)
        );

        memory.handle_value(ObjectMap::from([("test_key".into(), Value::from(5))]));

        assert_eq!(
            Ok(ObjectMap::from([
                ("key".into(), Value::from("test_key")),
                ("ttl".into(), Value::from(ttl)),
                ("value".into(), Value::from(5)),
            ])),
            memory.find_table_row(Case::Sensitive, &[condition], None, None, None)
        );
    }

    #[test]
    fn ignores_all_values_over_byte_size_limit() {
        let memory = Memory::new(build_memory_config(|c| {
            c.max_byte_size = Some(1);
        }));
        memory.handle_value(ObjectMap::from([("test_key".into(), Value::from(5))]));

        let condition = Condition::Equals {
            field: "key",
            value: Value::from("test_key"),
        };

        assert!(memory
            .find_table_rows(Case::Sensitive, &[condition], None, None, None)
            .unwrap()
            .pop()
            .is_none());
    }

    #[test]
    fn ignores_values_when_byte_size_limit_is_reached() {
        let ttl = 100;
        let memory = Memory::new(build_memory_config(|c| {
            c.ttl = ttl;
            c.max_byte_size = Some(150);
        }));
        memory.handle_value(ObjectMap::from([("test_key".into(), Value::from(5))]));
        memory.handle_value(ObjectMap::from([("rejected_key".into(), Value::from(5))]));

        assert_eq!(
            Ok(ObjectMap::from([
                ("key".into(), Value::from("test_key")),
                ("ttl".into(), Value::from(ttl)),
                ("value".into(), Value::from(5)),
            ])),
            memory.find_table_row(
                Case::Sensitive,
                &[Condition::Equals {
                    field: "key",
                    value: Value::from("test_key")
                }],
                None,
                None,
                None
            )
        );

        assert!(memory
            .find_table_rows(
                Case::Sensitive,
                &[Condition::Equals {
                    field: "key",
                    value: Value::from("rejected_key")
                }],
                None,
                None,
                None
            )
            .unwrap()
            .pop()
            .is_none());
    }

    #[test]
    fn missing_key() {
        let memory = Memory::new(Default::default());

        let condition = Condition::Equals {
            field: "key",
            value: Value::from("test_key"),
        };

        assert!(memory
            .find_table_rows(Case::Sensitive, &[condition], None, None, None)
            .unwrap()
            .pop()
            .is_none());
    }

    #[tokio::test]
    async fn sink_spec_compliance() {
        let event = Event::Log(LogEvent::from(ObjectMap::from([(
            "test_key".into(),
            Value::from(5),
        )])));

        let memory = Memory::new(Default::default());

        run_and_assert_sink_compliance(
            VectorSink::from_event_streamsink(memory),
            stream::once(ready(event)),
            &SINK_TAGS,
        )
        .await;
    }

    #[tokio::test]
    async fn flush_metrics_without_interval() {
        let event = Event::Log(LogEvent::from(ObjectMap::from([(
            "test_key".into(),
            Value::from(5),
        )])));

        let memory = Memory::new(Default::default());

        run_and_assert_sink_compliance(
            VectorSink::from_event_streamsink(memory),
            stream::once(ready(event)),
            &SINK_TAGS,
        )
        .await;

        let metrics = Controller::get().unwrap().capture_metrics();
        let insertions_counter = metrics
            .iter()
            .find(|m| {
                matches!(m.value(), MetricValue::Counter { .. })
                    && m.name() == "memory_enrichment_table_insertions_total"
            })
            .expect("Insertions metric is missing!");
        let MetricValue::Counter {
            value: insertions_count,
        } = insertions_counter.value()
        else {
            unreachable!();
        };
        let flushes_counter = metrics
            .iter()
            .find(|m| {
                matches!(m.value(), MetricValue::Counter { .. })
                    && m.name() == "memory_enrichment_table_flushes_total"
            })
            .expect("Flushes metric is missing!");
        let MetricValue::Counter {
            value: flushes_count,
        } = flushes_counter.value()
        else {
            unreachable!();
        };
        let object_count_gauge = metrics
            .iter()
            .find(|m| {
                matches!(m.value(), MetricValue::Gauge { .. })
                    && m.name() == "memory_enrichment_table_objects_count"
            })
            .expect("Object count metric is missing!");
        let MetricValue::Gauge {
            value: object_count,
        } = object_count_gauge.value()
        else {
            unreachable!();
        };
        let byte_size_gauge = metrics
            .iter()
            .find(|m| {
                matches!(m.value(), MetricValue::Gauge { .. })
                    && m.name() == "memory_enrichment_table_byte_size"
            })
            .expect("Byte size metric is missing!");
        assert_eq!(*insertions_count, 1.0);
        assert_eq!(*flushes_count, 1.0);
        assert_eq!(*object_count, 1.0);
        assert!(!byte_size_gauge.is_empty());
    }

    #[tokio::test]
    async fn flush_metrics_with_interval() {
        let event = Event::Log(LogEvent::from(ObjectMap::from([(
            "test_key".into(),
            Value::from(5),
        )])));

        let memory = Memory::new(build_memory_config(|c| {
            c.flush_interval = Some(1);
        }));

        run_and_assert_sink_compliance(
            VectorSink::from_event_streamsink(memory),
            stream::iter(vec![event.clone(), event]).flat_map(|e| {
                stream::once(async move {
                    tokio::time::sleep(Duration::from_millis(600)).await;
                    e
                })
            }),
            &SINK_TAGS,
        )
        .await;

        let metrics = Controller::get().unwrap().capture_metrics();
        let insertions_counter = metrics
            .iter()
            .find(|m| {
                matches!(m.value(), MetricValue::Counter { .. })
                    && m.name() == "memory_enrichment_table_insertions_total"
            })
            .expect("Insertions metric is missing!");
        let MetricValue::Counter {
            value: insertions_count,
        } = insertions_counter.value()
        else {
            unreachable!();
        };
        let flushes_counter = metrics
            .iter()
            .find(|m| {
                matches!(m.value(), MetricValue::Counter { .. })
                    && m.name() == "memory_enrichment_table_flushes_total"
            })
            .expect("Flushes metric is missing!");
        let MetricValue::Counter {
            value: flushes_count,
        } = flushes_counter.value()
        else {
            unreachable!();
        };
        let object_count_gauge = metrics
            .iter()
            .find(|m| {
                matches!(m.value(), MetricValue::Gauge { .. })
                    && m.name() == "memory_enrichment_table_objects_count"
            })
            .expect("Object count metric is missing!");
        let MetricValue::Gauge {
            value: object_count,
        } = object_count_gauge.value()
        else {
            unreachable!();
        };
        let byte_size_gauge = metrics
            .iter()
            .find(|m| {
                matches!(m.value(), MetricValue::Gauge { .. })
                    && m.name() == "memory_enrichment_table_byte_size"
            })
            .expect("Byte size metric is missing!");

        assert_eq!(*insertions_count, 2.0);
        // One is done right away and the next one after the interval
        assert_eq!(*flushes_count, 2.0);
        assert_eq!(*object_count, 1.0);
        assert!(!byte_size_gauge.is_empty());
    }

    #[tokio::test]
    async fn flush_metrics_with_key() {
        let event = Event::Log(LogEvent::from(ObjectMap::from([(
            "test_key".into(),
            Value::from(5),
        )])));

        let memory = Memory::new(build_memory_config(|c| {
            c.internal_metrics = InternalMetricsConfig {
                include_key_tag: true,
            };
        }));

        run_and_assert_sink_compliance(
            VectorSink::from_event_streamsink(memory),
            stream::once(ready(event)),
            &SINK_TAGS,
        )
        .await;

        let metrics = Controller::get().unwrap().capture_metrics();
        let insertions_counter = metrics
            .iter()
            .find(|m| {
                matches!(m.value(), MetricValue::Counter { .. })
                    && m.name() == "memory_enrichment_table_insertions_total"
            })
            .expect("Insertions metric is missing!");

        assert!(insertions_counter.tag_matches("key", "test_key"));
    }

    #[tokio::test]
    async fn flush_metrics_without_key() {
        let event = Event::Log(LogEvent::from(ObjectMap::from([(
            "test_key".into(),
            Value::from(5),
        )])));

        let memory = Memory::new(Default::default());

        run_and_assert_sink_compliance(
            VectorSink::from_event_streamsink(memory),
            stream::once(ready(event)),
            &SINK_TAGS,
        )
        .await;

        let metrics = Controller::get().unwrap().capture_metrics();
        let insertions_counter = metrics
            .iter()
            .find(|m| {
                matches!(m.value(), MetricValue::Counter { .. })
                    && m.name() == "memory_enrichment_table_insertions_total"
            })
            .expect("Insertions metric is missing!");

        assert!(insertions_counter.tag_value("key").is_none());
    }

    #[tokio::test]
    async fn source_spec_compliance() {
        let mut memory_config = MemoryConfig::default();
        memory_config.source_config = Some(MemorySourceConfig {
            export_interval: NonZeroU64::try_from(1).unwrap(),
            export_batch_size: None,
            remove_after_export: false,
            source_key: "test".to_string(),
        });
        let memory = memory_config.get_or_build_memory().await;
        memory.handle_value(ObjectMap::from([("test_key".into(), Value::from(5))]));

        let mut events: Vec<Event> = run_and_assert_source_compliance(
            memory_config,
            time::Duration::from_secs(5),
            &SOURCE_TAGS,
        )
        .await;

        assert!(!events.is_empty());
        let event = events.remove(0);
        let log = event.as_log();

        assert!(!log.value().is_empty());
    }
}
