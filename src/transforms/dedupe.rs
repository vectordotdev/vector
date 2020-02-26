use super::Transform;
use crate::{
    event::{Event, Value},
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::any::TypeId;
use std::collections::BTreeMap;
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub enum FieldMatchConfig {
    #[serde(rename = "match")]
    MatchFields(Vec<Atom>),
    #[serde(rename = "ignore")]
    IgnoreFields(Vec<Atom>),
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct CacheConfig {
    pub num_entries: usize,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct DedupeConfig {
    #[serde(default = "default_field_match_config")]
    pub fields: FieldMatchConfig,
    #[serde(default = "default_cache_config")]
    pub cache: CacheConfig,
}

fn default_cache_config() -> CacheConfig {
    CacheConfig { num_entries: 5000 }
}

fn default_field_match_config() -> FieldMatchConfig {
    FieldMatchConfig::MatchFields(vec!["timestamp".into()])
}

pub struct Dedupe {
    config: DedupeConfig,
    cache: LruCache<CacheEntry, bool>,
}

inventory::submit! {
    TransformDescription::new_without_default::<DedupeConfig>("dedupe")
}

#[typetag::serde(name = "dedupe")]
impl TransformConfig for DedupeConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(Dedupe::new(
            self.cache.num_entries,
            self.fields.clone(),
        )))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "dedupe"
    }
}

/// A CacheEntry comes in two forms, depending on the FieldMatchConfig in use.
///
/// When matching fields, a CacheEntry contains a vector of optional 2-tuples.  Each element in the
/// vector represents one field in the corresponding LogEvent.  Elements in the vector will
/// correspond 1:1 (and in order) to the fields specified in "fields.match".  The tuples each store
/// the TypeId for this field and the data as Bytes for the field.  There is no need to store the
/// field name because the elements of the vector correspond 1:1 to "fields.match", so there is
/// never any ambiguity about what field is being referred to.  If a field from "fields.match" does
/// not show up in an incoming Event, the CacheEntry will have None in the correspond location in
/// the vector.
///
/// When ignoring fields, a CacheEntry contains a vector of 3-tuples.  Each element in the vector
/// represents one field in the corresponding LogEvent.  The tuples will each contain the field
/// name, TypeId, and data as Bytes for the corresponding field (in that order).  Since the set of
/// fields that might go into CacheEntries is not known at startup, we must store the field names
/// as part of CacheEntries.  Since Event objects store their field in alphabetic order (as they
/// are backed by a BTreeMap), and we build CacheEntries by iterating over the fields of the
/// incoming Events, we know that the CacheEntries for 2 equivalent events will always contain the
/// fields in the same order.
#[derive(PartialEq, Eq, Hash)]
enum CacheEntry {
    Match(Vec<Option<(TypeId, Bytes)>>),
    Ignore(Vec<(Atom, TypeId, Bytes)>),
}

struct Null;

fn type_id_for_value(val: &Value) -> TypeId {
    match val {
        Value::Bytes(_) => TypeId::of::<Bytes>(),
        Value::Timestamp(_) => TypeId::of::<DateTime<Utc>>(),
        Value::Integer(_) => TypeId::of::<i64>(),
        Value::Float(_) => TypeId::of::<f64>(),
        Value::Boolean(_) => TypeId::of::<bool>(),
        Value::Map(_) => TypeId::of::<BTreeMap<Atom, Value>>(),
        Value::Array(_) => TypeId::of::<Vec<Value>>(),
        // There's no real type in Rust for Null, so we make a new type just for getting a unique
        // TypeId. All that matters is that all possible Value types have a distinct TypeId.
        Value::Null => TypeId::of::<Null>(),
    }
}

impl Dedupe {
    pub fn new(num_entries: usize, field_match_config: FieldMatchConfig) -> Self {
        Self {
            config: DedupeConfig {
                fields: field_match_config,
                cache: CacheConfig { num_entries },
            },
            cache: LruCache::new(num_entries),
        }
    }
}

/// Takes in an Event and returns a CacheEntry to place into the LRU cache containing
/// all relevant information for the fields that need matching against according to the
/// specified FieldMatchConfig.
fn build_cache_entry(event: &Event, fields: &FieldMatchConfig) -> CacheEntry {
    match &fields {
        FieldMatchConfig::MatchFields(fields) => {
            let mut entry = Vec::new();
            for field_name in fields.iter() {
                if let Some(value) = event.as_log().get(&field_name) {
                    entry.push(Some((type_id_for_value(&value), value.as_bytes())));
                } else {
                    entry.push(None);
                }
            }
            CacheEntry::Match(entry)
        }
        FieldMatchConfig::IgnoreFields(fields) => {
            let mut entry = Vec::new();

            for (field_name, value) in event.as_log().all_fields() {
                if !fields.contains(&field_name) {
                    entry.push((
                        field_name.clone(),
                        type_id_for_value(&value),
                        value.as_bytes(),
                    ));
                }
            }

            CacheEntry::Ignore(entry)
        }
    }
}

impl Transform for Dedupe {
    fn transform(&mut self, event: Event) -> Option<Event> {
        let cache_entry = build_cache_entry(&event, &self.config.fields);
        if self.cache.put(cache_entry, true).is_some() {
            None
        } else {
            Some(event)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Dedupe;
    use crate::transforms::dedupe::FieldMatchConfig;
    use crate::{event::Event, event::Value, transforms::Transform};
    use std::collections::BTreeMap;
    use string_cache::DefaultAtom as Atom;

    fn make_match_config(fields: Vec<Atom>) -> FieldMatchConfig {
        FieldMatchConfig::MatchFields(fields)
    }

    fn make_ignore_config(given_fields: Vec<Atom>) -> FieldMatchConfig {
        // "message" and "timestamp" are added automatically to all Events
        let mut fields = vec!["message".into(), "timestamp".into()];
        fields.extend(given_fields);

        FieldMatchConfig::IgnoreFields(fields)
    }

    #[test]
    fn dedupe_match_basic() {
        let transform = Dedupe::new(5, make_match_config(vec!["matched".into()]));
        basic(transform);
    }

    #[test]
    fn dedupe_ignore_basic() {
        let transform = Dedupe::new(5, make_ignore_config(vec!["unmatched".into()]));
        basic(transform);
    }

    fn basic(mut transform: Dedupe) {
        let mut event1 = Event::from("message");
        event1.as_mut_log().insert("matched", "some value");
        event1.as_mut_log().insert("unmatched", "another value");

        // Test that unmatched field isn't considered
        let mut event2 = Event::from("message");
        event2.as_mut_log().insert("matched", "some value2");
        event2.as_mut_log().insert("unmatched", "another value");

        // Test that matched field is considered
        let mut event3 = Event::from("message");
        event3.as_mut_log().insert("matched", "some value");
        event3.as_mut_log().insert("unmatched", "another value2");

        // First event should always be passed through as-is.
        let new_event = transform.transform(event1).unwrap();
        assert_eq!(new_event.as_log()[&"matched".into()], "some value".into());

        // Second event differs in matched field so should be outputted even though it
        // has the same value for unmatched field.
        let new_event = transform.transform(event2).unwrap();
        assert_eq!(new_event.as_log()[&"matched".into()], "some value2".into());

        // Third event has the same value for "matched" as first event, so it should be dropped.
        assert_eq!(None, transform.transform(event3));
    }

    #[test]
    fn dedupe_match_field_name_matters() {
        let transform = Dedupe::new(
            5,
            make_match_config(vec!["matched1".into(), "matched2".into()]),
        );
        field_name_matters(transform);
    }

    #[test]
    fn dedupe_ignore_field_name_matters() {
        let transform = Dedupe::new(5, make_ignore_config(vec![]));
        field_name_matters(transform);
    }

    fn field_name_matters(mut transform: Dedupe) {
        let mut event1 = Event::from("message");
        event1.as_mut_log().insert("matched1", "some value");

        let mut event2 = Event::from("message");
        event2.as_mut_log().insert("matched2", "some value");

        // First event should always be passed through as-is.
        let new_event = transform.transform(event1).unwrap();
        assert_eq!(new_event.as_log()[&"matched1".into()], "some value".into());

        // Second event has a different matched field name with the same value, so it should not be
        // considered a dupe
        let new_event = transform.transform(event2).unwrap();
        assert_eq!(new_event.as_log()[&"matched2".into()], "some value".into());
    }

    #[test]
    fn dedupe_match_field_order_irrelevant() {
        let transform = Dedupe::new(
            5,
            make_match_config(vec!["matched1".into(), "matched2".into()]),
        );
        field_order_irrelevant(transform);
    }

    #[test]
    fn dedupe_ignore_field_order_irrelevant() {
        let transform = Dedupe::new(5, make_ignore_config(vec!["randomData".into()]));
        field_order_irrelevant(transform);
    }

    /// Test that two Events that are considered duplicates get handled that way, even
    /// if the order of the matched fields is different between the two.
    fn field_order_irrelevant(mut transform: Dedupe) {
        let mut event1 = Event::from("message");
        event1.as_mut_log().insert("matched1", "value1");
        event1.as_mut_log().insert("matched2", "value2");

        // Add fields in opposite order
        let mut event2 = Event::from("message");
        event2.as_mut_log().insert("matched2", "value2");
        event2.as_mut_log().insert("matched1", "value1");

        // First event should always be passed through as-is.
        let new_event = transform.transform(event1).unwrap();
        assert_eq!(new_event.as_log()[&"matched1".into()], "value1".into());
        assert_eq!(new_event.as_log()[&"matched2".into()], "value2".into());

        // Second event is the same just with different field order, so it shouldn't be outputted.
        assert_eq!(None, transform.transform(event2));
    }

    #[test]
    fn dedupe_match_age_out() {
        // Construct transform with a cache size of only 1 entry.
        let transform = Dedupe::new(1, FieldMatchConfig::MatchFields(vec!["matched".into()]));
        age_out(transform);
    }

    #[test]
    fn dedupe_ignore_age_out() {
        // Construct transform with a cache size of only 1 entry.
        let transform = Dedupe::new(1, make_ignore_config(vec![]));
        age_out(transform);
    }

    /// Test the eviction behavior of the underlying LruCache
    fn age_out(mut transform: Dedupe) {
        let mut event1 = Event::from("message");
        event1.as_mut_log().insert("matched", "some value");

        let mut event2 = Event::from("message");
        event2.as_mut_log().insert("matched", "some value2");

        // This event is a duplicate of event1, but won't be treated as such.
        let event3 = event1.clone();

        // First event should always be passed through as-is.
        let new_event = transform.transform(event1).unwrap();
        assert_eq!(new_event.as_log()[&"matched".into()], "some value".into());

        // Second event gets outputted because it's not a dupe.  This causes the first
        // Event to be evicted from the cache.
        let new_event = transform.transform(event2).unwrap();
        assert_eq!(new_event.as_log()[&"matched".into()], "some value2".into());

        // Third event is a dupe but gets outputted anyway because the first event has aged
        // out of the cache.
        let new_event = transform.transform(event3).unwrap();
        assert_eq!(new_event.as_log()[&"matched".into()], "some value".into());
    }

    #[test]
    fn dedupe_match_type_matching() {
        let transform = Dedupe::new(5, make_match_config(vec!["matched".into()]));
        type_matching(transform);
    }

    #[test]
    fn dedupe_ignore_type_matching() {
        let transform = Dedupe::new(5, make_ignore_config(vec![]));
        type_matching(transform);
    }

    /// Test that two events with values for the matched fields that have different
    /// types but the same string representation aren't considered duplicates.
    fn type_matching(mut transform: Dedupe) {
        let mut event1 = Event::from("message");
        event1.as_mut_log().insert("matched", "123");

        let mut event2 = Event::from("message");
        event2.as_mut_log().insert("matched", 123);

        // First event should always be passed through as-is.
        let new_event = transform.transform(event1).unwrap();
        assert_eq!(new_event.as_log()[&"matched".into()], "123".into());

        // Second event should also get passed through even though the string representations of
        // "matched" are the same.
        let new_event = transform.transform(event2).unwrap();
        assert_eq!(new_event.as_log()[&"matched".into()], 123.into());
    }

    #[test]
    fn dedupe_match_type_matching_nested_objects() {
        let transform = Dedupe::new(5, make_match_config(vec!["matched".into()]));
        type_matching_nested_objects(transform);
    }

    #[test]
    fn dedupe_ignore_type_matching_nested_objects() {
        let transform = Dedupe::new(5, make_ignore_config(vec![]));
        type_matching_nested_objects(transform);
    }

    /// Test that two events where the matched field is a sub object and that object contains values
    /// that have different types but the same string representation aren't considered duplicates.
    fn type_matching_nested_objects(mut transform: Dedupe) {
        let mut map1: BTreeMap<Atom, Value> = BTreeMap::new();
        map1.insert("key".into(), "123".into());
        let mut event1 = Event::from("message");
        event1.as_mut_log().insert("matched", map1);

        let mut map2: BTreeMap<Atom, Value> = BTreeMap::new();
        map2.insert("key".into(), 123.into());
        let mut event2 = Event::from("message");
        event2.as_mut_log().insert("matched", map2);

        // First event should always be passed through as-is.
        let new_event = transform.transform(event1).unwrap();
        let res_value = new_event.as_log()[&"matched".into()].clone();
        if let Value::Map(map) = res_value {
            assert_eq!(map.get(&"key".into()).unwrap().clone(), Value::from("123"));
        }

        // Second event should also get passed through even though the string representations of
        // "matched" are the same.
        let new_event = transform.transform(event2).unwrap();
        let res_value = new_event.as_log()[&"matched".into()].clone();
        if let Value::Map(map) = res_value {
            assert_eq!(map.get(&"key".into()).unwrap().clone(), Value::from(123));
        }
    }
}
