//! Handles enrichment tables for `type = memory`.
use std::sync::{Arc, Mutex};
use std::time::Instant;

use evmap::shallow_copy::CopyValue;
use evmap::{self};
use evmap_derive::ShallowCopy;
use thread_local::ThreadLocal;
use vector_lib::EstimatedJsonEncodedSizeOf;

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::BoxStream;
use tokio_stream::StreamExt;
use vector_lib::configurable::configurable_component;
use vector_lib::enrichment::{Case, Condition, IndexHandle, Table};
use vector_lib::event::{Event, EventStatus, Finalizable};
use vector_lib::internal_event::{
    ByteSize, BytesSent, CountByteSize, EventsSent, InternalEventHandle, Output, Protocol,
};
use vector_lib::sink::StreamSink;
use vrl::value::{KeyString, ObjectMap, Value};

use crate::config::EnrichmentTableConfig;

/// Configuration for the `memory` enrichment table.
#[configurable_component(enrichment_table("memory"))]
#[derive(Clone, Eq, PartialEq)]
pub struct MemoryConfig {
    /// TTL (time-to-live), used to limit lifetime of data stored in cache.
    /// When TTL expires, data behind a specific key in cache is removed.
    /// TTL is reset when replacing the key.
    #[serde(default = "default_ttl")]
    ttl: u64,
    /// Scan interval for updating TTL of keys in seconds. This is provided
    /// as an optimization, to ensure that TTL is updated, but without doing
    /// too many cache scans.
    #[serde(default = "default_scan_interval")]
    scan_interval: u64,
    /// Interval for making writes visible in the table.
    /// Longer interval might get better performance,
    /// but data would be visible in the table after a longer delay.
    /// Since every TTL scan makes its changes visible, this value
    /// only makes sense if it is shorter than scan_interval
    ///
    /// By default, all writes are made visible immediately.
    #[serde(default = "default_write_refresh_interval")]
    write_refresh_interval: u64,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            ttl: default_ttl(),
            scan_interval: default_scan_interval(),
            write_refresh_interval: default_write_refresh_interval(),
        }
    }
}

const fn default_ttl() -> u64 {
    600
}

const fn default_scan_interval() -> u64 {
    30
}

const fn default_write_refresh_interval() -> u64 {
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
            .field("write_refresh_interval", &self.write_refresh_interval)
            .finish()
    }
}

impl_generate_config_from_default!(MemoryConfig);

/// Single memory entry containing the value and TTL
#[derive(Clone, Eq, PartialEq, Hash, ShallowCopy)]
pub struct MemoryEntry {
    key: String,
    value: Box<Value>,
    update_time: CopyValue<Instant>,
}

impl MemoryEntry {
    fn into_object_map(&self, now: Instant, total_ttl: u64) -> ObjectMap {
        let ttl = total_ttl
            .checked_sub(now.duration_since(*self.update_time).as_secs())
            .unwrap_or(0);
        ObjectMap::from([
            (
                KeyString::from("key"),
                Value::Bytes(Bytes::copy_from_slice(self.key.as_bytes())),
            ),
            (KeyString::from("value"), (*self.value).clone()),
            (
                KeyString::from("ttl"),
                Value::Integer(ttl.try_into().unwrap_or(i64::MAX)),
            ),
        ])
    }

    fn expired(&self, now: Instant, ttl: u64) -> bool {
        return now.duration_since(*self.update_time).as_secs() > ttl;
    }
}

/// A struct that implements [vector_lib::enrichment::Table] to handle loading enrichment data from a memory structure.
pub struct Memory {
    read_handle_factory: evmap::ReadHandleFactory<String, MemoryEntry>,
    read_handle: ThreadLocal<evmap::ReadHandle<String, MemoryEntry>>,
    write_handle: Arc<Mutex<evmap::WriteHandle<String, MemoryEntry>>>,
    config: MemoryConfig,
    last_ttl_scan: Instant,
    last_write_refresh: Instant,
}

impl Memory {
    /// Creates a new [Memory] based on the provided config.
    pub fn new(config: MemoryConfig) -> Self {
        let (read_handle, write_handle) = evmap::new();
        Self {
            config,
            read_handle_factory: read_handle.factory(),
            read_handle: ThreadLocal::new(),
            write_handle: Arc::new(Mutex::new(write_handle)),
            last_ttl_scan: Instant::now(),
            last_write_refresh: Instant::now(),
        }
    }

    fn get_read_handle(&self) -> &evmap::ReadHandle<String, MemoryEntry> {
        self.read_handle
            .get_or(|| self.read_handle_factory.handle())
    }

    fn handle_value(&mut self, value: &ObjectMap) {
        // Panic: If the Mutex is poisoned
        let mut handle = self.write_handle.lock().unwrap();
        let now = Instant::now();

        for (k, v) in value.iter() {
            handle.update(
                k.as_str().to_string(),
                MemoryEntry {
                    key: k.as_str().to_string(),
                    value: Box::new(v.clone()),
                    update_time: now.into(),
                },
            );
        }

        let mut needs_refresh = false;
        if now.duration_since(self.last_ttl_scan).as_secs() >= self.config.scan_interval {
            self.last_ttl_scan = now;
            // Since evmap holds 2 separate maps for the data, we are free to directly remove
            // elements via the writer, while we are iterating the reader
            // Refresh will happen only after we manually invoke it after iteration
            if let Some(reader) = self.get_read_handle().read() {
                for (k, v) in reader.iter() {
                    if let Some(entry) = v.get_one() {
                        if entry.expired(now, self.config.ttl) {
                            handle.empty(k.clone());
                            needs_refresh = true;
                        }
                    }
                }
            };
        } else if now.duration_since(self.last_write_refresh).as_secs()
            >= self.config.write_refresh_interval
        {
            needs_refresh = true;
        }

        if needs_refresh {
            self.last_write_refresh = now;
            handle.refresh();
        }
    }
}

impl Clone for Memory {
    fn clone(&self) -> Self {
        Self {
            read_handle_factory: self.read_handle_factory.clone(),
            read_handle: ThreadLocal::new(),
            write_handle: self.write_handle.clone(),
            config: self.config.clone(),
            last_ttl_scan: self.last_ttl_scan.clone(),
            last_write_refresh: self.last_write_refresh.clone(),
        }
    }
}

impl Table for Memory {
    fn find_table_row<'a>(
        &self,
        case: Case,
        condition: &'a [Condition<'a>],
        select: Option<&'a [String]>,
        index: Option<IndexHandle>,
    ) -> Result<ObjectMap, String> {
        let mut rows = self.find_table_rows(case, condition, select, index)?;

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
        _index: Option<IndexHandle>,
    ) -> Result<Vec<ObjectMap>, String> {
        match condition.first() {
            Some(_) if condition.len() > 1 => Err("Only one condition is allowed".to_string()),
            Some(Condition::Equals { value, .. }) => {
                let key = value.to_string_lossy();
                match self.get_read_handle().get_one(key.as_ref()) {
                    Some(row) => Ok(vec![row.into_object_map(Instant::now(), self.config.ttl)]),
                    None => Ok(Default::default()),
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
        while let Some(mut event) = input.next().await {
            let event_byte_size = event.estimated_json_encoded_size_of();

            let finalizers = event.take_finalizers();

            // Panic: This sink only accepts Logs, so this should never panic
            let log = event.into_log();

            match log.value() {
                Value::Object(map) => self.handle_value(map),
                _ => (),
            };

            finalizers.update_status(EventStatus::Delivered);
            events_sent.emit(CountByteSize(1, event_byte_size));
            bytes_sent.emit(ByteSize(event_byte_size.get()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use futures::future::ready;
    use futures_util::stream;
    use std::time::Duration;

    use vector_lib::sink::VectorSink;

    use super::*;
    use crate::{
        event::{Event, LogEvent},
        test_util::components::{run_and_assert_sink_compliance, SINK_TAGS},
    };

    #[test]
    fn finds_row() {
        let mut memory = Memory::new(Default::default());
        memory.handle_value(&ObjectMap::from([("test_key".into(), Value::from(5))]));

        let condition = Condition::Equals {
            field: "key",
            value: Value::from("test_key"),
        };

        assert_eq!(
            Ok(ObjectMap::from([
                ("key".into(), Value::from("test_key")),
                ("ttl".into(), Value::from(default_ttl())),
                ("value".into(), Value::from(5)),
            ])),
            memory.find_table_row(Case::Sensitive, &[condition], None, None)
        );
    }

    #[test]
    fn calculates_ttl() {
        let ttl = 100;
        let secs_to_subtract = 10;
        let memory = Memory::new(MemoryConfig {
            ttl,
            ..Default::default()
        });
        {
            let mut handle = memory.write_handle.lock().unwrap();
            handle.update(
                "test_key".to_string(),
                MemoryEntry {
                    key: "test_key".to_string(),
                    value: Box::new(Value::from(5)),
                    update_time: (Instant::now() - Duration::from_secs(secs_to_subtract)).into(),
                },
            );
            handle.refresh();
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
            memory.find_table_row(Case::Sensitive, &[condition], None, None)
        );
    }

    #[test]
    fn removes_expired_records_on_scan_interval() {
        let ttl = 100;
        let mut memory = Memory::new(MemoryConfig {
            ttl,
            scan_interval: 0,
            ..Default::default()
        });
        {
            let mut handle = memory.write_handle.lock().unwrap();
            handle.update(
                "test_key".to_string(),
                MemoryEntry {
                    key: "test_key".to_string(),
                    value: Box::new(Value::from(5)),
                    update_time: (Instant::now() - Duration::from_secs(ttl + 10)).into(),
                },
            );
            handle.refresh();
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
            memory.find_table_row(Case::Sensitive, &[condition.clone()], None, None)
        );

        // Force scan
        memory.handle_value(&ObjectMap::default());

        // The value is not present anymore
        assert!(memory
            .find_table_rows(Case::Sensitive, &[condition], None, None)
            .unwrap()
            .pop()
            .is_none());
    }

    #[test]
    fn does_not_show_values_before_refresh_interval() {
        let ttl = 100;
        let mut memory = Memory::new(MemoryConfig {
            ttl,
            write_refresh_interval: 10,
            ..Default::default()
        });
        memory.handle_value(&ObjectMap::from([("test_key".into(), Value::from(5))]));

        let condition = Condition::Equals {
            field: "key",
            value: Value::from("test_key"),
        };

        assert!(memory
            .find_table_rows(Case::Sensitive, &[condition], None, None)
            .unwrap()
            .pop()
            .is_none());
    }

    #[test]
    fn updates_ttl_on_value_replacement() {
        let ttl = 100;
        let mut memory = Memory::new(MemoryConfig {
            ttl,
            ..Default::default()
        });
        {
            let mut handle = memory.write_handle.lock().unwrap();
            handle.update(
                "test_key".to_string(),
                MemoryEntry {
                    key: "test_key".to_string(),
                    value: Box::new(Value::from(5)),
                    update_time: (Instant::now() - Duration::from_secs(ttl / 2)).into(),
                },
            );
            handle.refresh();
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
            memory.find_table_row(Case::Sensitive, &[condition.clone()], None, None)
        );

        memory.handle_value(&ObjectMap::from([("test_key".into(), Value::from(5))]));

        assert_eq!(
            Ok(ObjectMap::from([
                ("key".into(), Value::from("test_key")),
                ("ttl".into(), Value::from(ttl)),
                ("value".into(), Value::from(5)),
            ])),
            memory.find_table_row(Case::Sensitive, &[condition], None, None)
        );
    }

    #[test]
    fn missing_key() {
        let memory = Memory::new(Default::default());

        let condition = Condition::Equals {
            field: "key",
            value: Value::from("test_key"),
        };

        assert!(memory
            .find_table_rows(Case::Sensitive, &[condition], None, None)
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
}
