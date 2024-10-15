//! Handles enrichment tables for `type = memory`.
use std::sync::{Arc, Mutex};

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
use vector_lib::internal_event::{CountByteSize, EventsSent, InternalEventHandle, Output};
use vector_lib::sink::StreamSink;
use vrl::value::{KeyString, ObjectMap, Value};

use crate::config::EnrichmentTableConfig;

/// Configuration for the `memory` enrichment table.
#[configurable_component(enrichment_table("memory"))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryConfig {
    /// TTL (time-to-live), used to limit lifetime of data stored in cache.
    /// When TTL expires, data behind a specific key in cache is removed.
    /// TTL is restarted when using the key.
    #[serde(default = "default_ttl")]
    ttl: u64,
    /// Scan interval for updating TTL of keys in seconds. This is provided
    /// as an optimization, to ensure that TTL is updated, but without doing
    /// too many cache scans.
    #[serde(default = "default_scan_interval")]
    scan_interval: u64,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            ttl: default_ttl(),
            scan_interval: default_scan_interval(),
        }
    }
}

const fn default_ttl() -> u64 {
    600
}

const fn default_scan_interval() -> u64 {
    30
}

impl EnrichmentTableConfig for MemoryConfig {
    async fn build(
        &self,
        _globals: &crate::config::GlobalOptions,
    ) -> crate::Result<Box<dyn Table + Send + Sync>> {
        Ok(Box::new(Memory::new(self.clone())))
    }
}

impl_generate_config_from_default!(MemoryConfig);

/// Single memory entry containing the value and TTL
#[derive(Clone, Eq, PartialEq, Hash, ShallowCopy)]
pub struct MemoryEntry {
    key: String,
    value: Box<Value>,
    ttl: i64,
}

impl MemoryEntry {
    fn into_object_map(&self) -> ObjectMap {
        ObjectMap::from([
            (
                KeyString::from("key"),
                Value::Bytes(Bytes::copy_from_slice(self.key.as_bytes())),
            ),
            (KeyString::from("value"), (*self.value).clone()),
            (KeyString::from("ttl"), Value::Integer(self.ttl)),
        ])
    }
}

/// A struct that implements [vector_lib::enrichment::Table] to handle loading enrichment data from a memory structure.
pub struct Memory {
    read_handle_factory: evmap::ReadHandleFactory<String, MemoryEntry>,
    read_handle: ThreadLocal<evmap::ReadHandle<String, MemoryEntry>>,
    write_handle: Arc<Mutex<evmap::WriteHandle<String, MemoryEntry>>>,
    _config: MemoryConfig,
}

impl Memory {
    /// Creates a new [Memory] based on the provided config.
    pub fn new(config: MemoryConfig) -> Self {
        let (read_handle, write_handle) = evmap::new();
        Self {
            _config: config,
            read_handle_factory: read_handle.factory(),
            read_handle: ThreadLocal::new(),
            write_handle: Arc::new(Mutex::new(write_handle)),
        }
    }
}

impl Clone for Memory {
    fn clone(&self) -> Self {
        Self {
            read_handle_factory: self.read_handle_factory.clone(),
            read_handle: ThreadLocal::new(),
            write_handle: self.write_handle.clone(),
            _config: self._config.clone(),
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
                match self
                    .read_handle
                    .get_or(|| self.read_handle_factory.handle())
                    .get_one(key.as_ref())
                {
                    Some(row) => Ok(vec![row.into_object_map()]),
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
        write!(
            f,
            "Memory {} row(s)",
            self.read_handle
                .get_or(|| self.read_handle_factory.handle())
                .len()
        )
    }
}

#[async_trait]
impl StreamSink<Event> for Memory {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        let events_sent = register!(EventsSent::from(Output(None)));
        while let Some(mut event) = input.next().await {
            let event_byte_size = event.estimated_json_encoded_size_of();

            let finalizers = event.take_finalizers();

            // Panic: This sink only accepts Logs, so this should never panic
            let log = event.into_log();

            match log.value() {
                Value::Object(map) => {
                    // Panic: If the Mutex is poisoned
                    let mut handle = self.write_handle.lock().unwrap();
                    for (k, v) in map.iter() {
                        handle.update(
                            k.as_str().to_string(),
                            MemoryEntry {
                                key: k.as_str().to_string(),
                                value: Box::new(v.clone()),
                                ttl: 500,
                            },
                        );
                    }
                    handle.refresh();
                }
                _ => (),
            };

            finalizers.update_status(EventStatus::Delivered);
            events_sent.emit(CountByteSize(1, event_byte_size));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_row() {
        let memory = Memory::new(Default::default());
        {
            let mut handle = memory.write_handle.lock().unwrap();
            handle.update(
                "test_key".to_string(),
                MemoryEntry {
                    key: "test_key".to_string(),
                    value: Box::new(Value::Integer(5)),
                    ttl: 500,
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
                ("ttl".into(), Value::from(500)),
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
}
