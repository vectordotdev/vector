//! Handles enrichment tables for `type = memory`.
use std::collections::BTreeMap;

use bytes::Bytes;
use vector_lib::configurable::configurable_component;
use vector_lib::enrichment::{Case, Condition, IndexHandle, Table};
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
#[derive(Clone)]
pub struct MemoryEntry {
    key: String,
    value: Value,
    ttl: i64,
}

impl MemoryEntry {
    fn into_object_map(&self) -> ObjectMap {
        ObjectMap::from([
            (
                KeyString::from("key"),
                Value::Bytes(Bytes::copy_from_slice(self.key.as_bytes())),
            ),
            (KeyString::from("value"), self.value.clone()),
            (KeyString::from("ttl"), Value::Integer(self.ttl)),
        ])
    }
}

/// A struct that implements [vector_lib::enrichment::Table] to handle loading enrichment data from a memory structure.
#[derive(Clone)]
pub struct Memory {
    data: BTreeMap<String, MemoryEntry>,
    _config: MemoryConfig,
}

impl Memory {
    /// Creates a new [Memory] based on the provided config.
    pub fn new(config: MemoryConfig) -> Self {
        Self {
            _config: config,
            data: Default::default(),
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
                match self.data.get(key.as_ref()) {
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
        write!(f, "Memory {} row(s)", self.data.len(),)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_row() {
        let mut memory = Memory::new(Default::default());
        memory.data = BTreeMap::from([(
            "test_key".to_string(),
            MemoryEntry {
                key: "test_key".to_string(),
                value: Value::Integer(5),
                ttl: 500,
            },
        )]);

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
