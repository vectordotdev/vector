use std::{future::ready, num::NonZeroUsize, pin::Pin};

use bytes::Bytes;
use futures::{Stream, StreamExt};
use lru::LruCache;
use vector_lib::lookup::lookup_v2::ConfigTargetPath;
use vrl::path::OwnedTargetPath;

use crate::{
    event::{Event, Value},
    internal_events::DedupeEventsDropped,
    transforms::TaskTransform,
};

use super::common::FieldMatchConfig;

#[derive(Clone)]
pub struct Dedupe {
    fields: FieldMatchConfig,
    cache: LruCache<CacheEntry, bool>,
}

type TypeId = u8;

/// A CacheEntry comes in two forms, depending on the FieldMatchConfig in use.
///
/// When matching fields, a CacheEntry contains a vector of optional 2-tuples.
/// Each element in the vector represents one field in the corresponding
/// LogEvent. Elements in the vector will correspond 1:1 (and in order) to the
/// fields specified in "fields.match". The tuples each store the TypeId for
/// this field and the data as Bytes for the field. There is no need to store
/// the field name because the elements of the vector correspond 1:1 to
/// "fields.match", so there is never any ambiguity about what field is being
/// referred to. If a field from "fields.match" does not show up in an incoming
/// Event, the CacheEntry will have None in the correspond location in the
/// vector.
///
/// When ignoring fields, a CacheEntry contains a vector of 3-tuples. Each
/// element in the vector represents one field in the corresponding LogEvent.
/// The tuples will each contain the field name, TypeId, and data as Bytes for
/// the corresponding field (in that order). Since the set of fields that might
/// go into CacheEntries is not known at startup, we must store the field names
/// as part of CacheEntries. Since Event objects store their field in alphabetic
/// order (as they are backed by a BTreeMap), and we build CacheEntries by
/// iterating over the fields of the incoming Events, we know that the
/// CacheEntries for 2 equivalent events will always contain the fields in the
/// same order.
#[derive(Clone, PartialEq, Eq, Hash)]
enum CacheEntry {
    Match(Vec<Option<(TypeId, Bytes)>>),
    Ignore(Vec<(OwnedTargetPath, TypeId, Bytes)>),
}

/// Assigns a unique number to each of the types supported by Event::Value.
const fn type_id_for_value(val: &Value) -> TypeId {
    match val {
        Value::Bytes(_) => 0,
        Value::Timestamp(_) => 1,
        Value::Integer(_) => 2,
        Value::Float(_) => 3,
        Value::Boolean(_) => 4,
        Value::Object(_) => 5,
        Value::Array(_) => 6,
        Value::Null => 7,
        Value::Regex(_) => 8,
    }
}

impl Dedupe {
    pub fn new(num_entries: NonZeroUsize, fields: FieldMatchConfig) -> Self {
        Self {
            fields,
            cache: LruCache::new(num_entries),
        }
    }

    pub fn transform_one(&mut self, event: Event) -> Option<Event> {
        let cache_entry = build_cache_entry(&event, &self.fields);
        if self.cache.put(cache_entry, true).is_some() {
            emit!(DedupeEventsDropped { count: 1 });
            None
        } else {
            Some(event)
        }
    }
}

/// Takes in an Event and returns a CacheEntry to place into the LRU cache
/// containing all relevant information for the fields that need matching
/// against according to the specified FieldMatchConfig.
fn build_cache_entry(event: &Event, fields: &FieldMatchConfig) -> CacheEntry {
    match &fields {
        FieldMatchConfig::MatchFields(fields) => {
            let mut entry = Vec::new();
            for field_name in fields.iter() {
                if let Some(value) = event.as_log().get(field_name) {
                    entry.push(Some((type_id_for_value(value), value.coerce_to_bytes())));
                } else {
                    entry.push(None);
                }
            }
            CacheEntry::Match(entry)
        }
        FieldMatchConfig::IgnoreFields(fields) => {
            let mut entry = Vec::new();

            if let Some(event_fields) = event.as_log().all_event_fields() {
                if let Some(metadata_fields) = event.as_log().all_metadata_fields() {
                    for (field_name, value) in event_fields.chain(metadata_fields) {
                        if let Ok(path) = ConfigTargetPath::try_from(field_name) {
                            if !fields.contains(&path) {
                                entry.push((
                                    path.0,
                                    type_id_for_value(value),
                                    value.coerce_to_bytes(),
                                ));
                            }
                        }
                    }
                }
            }

            CacheEntry::Ignore(entry)
        }
    }
}

impl TaskTransform<Event> for Dedupe {
    fn transform(
        self: Box<Self>,
        task: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static,
    {
        let mut inner = self;
        Box::pin(task.filter_map(move |v| ready(inner.transform_one(v))))
    }
}

#[cfg(test)]
mod tests {
    use crate::event::LogEvent;
    use crate::test_util::components::assert_transform_compliance;
    use crate::transforms::dedupe::common::{default_cache_config, FieldMatchConfig};
    use crate::transforms::dedupe::config::DedupeConfig;
    use crate::transforms::test::create_topology;
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;
    use vector_lib::lookup::lookup_v2::ConfigTargetPath;
    use vrl::value::Value;

    pub fn assert_eq_values(left: LogEvent, right: LogEvent) {
        let inner_left = left.into_parts().0;
        let inner_right = right.into_parts().0;
        assert_eq!(inner_left, inner_right);
    }

    #[tokio::test]
    async fn default_match() {
        let config = DedupeConfig {
            cache: default_cache_config(),
            fields: None,
        };

        assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(1);

            let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

            let event1 = LogEvent::from(btreemap! {
                "message" => "foo",
                "host" => "bar",
                "timestamp" => "t1",
            });
            tx.send(event1.clone().into()).await.unwrap();
            let output = out.recv().await.unwrap().into_log();
            assert_eq_values(event1.clone(), output);

            let event2 = event1.clone();
            tx.send(event2.into()).await.unwrap();

            let mut event3 = event1.clone();
            event3.insert("message", Value::from("another"));
            tx.send(event3.clone().into()).await.unwrap();

            let output = out.recv().await.unwrap().into_log();
            assert_eq_values(event3, output);

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await
    }

    #[tokio::test]
    async fn custom_match() {
        let config = DedupeConfig {
            cache: default_cache_config(),
            fields: Some(FieldMatchConfig::MatchFields(vec![
                ConfigTargetPath::from("a"),
                ConfigTargetPath::from("b"),
            ])),
        };

        assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(1);

            let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

            let event1 = LogEvent::from(btreemap! {
                "message" => "foo",
                "a" => 1,
                "b" => 2,
            });
            tx.send(event1.clone().into()).await.unwrap();
            let output = out.recv().await.unwrap().into_log();
            assert_eq_values(event1.clone(), output);

            let event2 = event1.clone();
            tx.send(event2.into()).await.unwrap();

            let event3 = LogEvent::from(btreemap! {
                "message" => "bar",
                "a" => 3,
                "b" => 2,
            });
            tx.send(event3.clone().into()).await.unwrap();
            let output = out.recv().await.unwrap().into_log();
            assert_eq_values(event3, output);

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await
    }
}
