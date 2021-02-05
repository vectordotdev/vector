use serde::{ser::SerializeMap, Serialize, Serializer};
use std::collections::HashMap;

/// Use a sorted Vec to store key-value pairs.
/// Useful as a replacement of HashMap/BTreeMap where:
/// - There is no need for fast access of values by keys.
/// - The keys need to be sorted to be use as partition key
/// for PartitionBatchSink.
///
/// Ideal to store labels map in various sinks.
#[derive(Debug, Clone)]
pub struct VecMap<V> {
    pub entries: Vec<(String, V)>,
}

impl<V> From<HashMap<String, V>> for VecMap<V> {
    fn from(map: HashMap<String, V>) -> Self {
        let mut entries = map.into_iter().collect::<Vec<_>>();
        // keys are unique because they come from HashMap,
        // therefore no need to compare values.
        entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        Self { entries }
    }
}

/// Useful for serialization of partition keys without having to
/// build a HashMap/BTreeMap.
pub struct SliceMap<'a, K, V> {
    pub entries: &'a [(K, V)],
}

impl<'a, K, V> Serialize for SliceMap<'a, K, V>
where
    K: Serialize,
    V: Serialize,
{
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.entries.len()))?;
        for (k, v) in self.entries {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

impl<'a, K, V> SliceMap<'a, K, V> {
    pub fn new(entries: &'a [(K, V)]) -> Self {
        Self { entries }
    }
}
