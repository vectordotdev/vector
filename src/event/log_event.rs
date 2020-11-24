use crate::event::{
    lookup::{Segment, SegmentBuf},
    Lookup, LookupBuf, Value,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    collections::{btree_map::Entry, BTreeMap, HashMap},
    convert::{TryFrom, TryInto},
    fmt::Debug,
    iter::FromIterator,
};
use std::ops::IndexMut;

#[derive(PartialEq, Debug, Clone, Default)]
pub struct LogEvent {
    fields: BTreeMap<String, Value>,
}

impl LogEvent {
    /// Get an immutable borrow of the given value by lookup.
    #[instrument(level = "trace", skip(self))]
    pub fn get<'a>(&self, lookup: impl Into<Lookup<'a>> + Debug) -> Option<&Value> {
        let lookup = lookup.into();
        let mut lookup_iter = lookup.iter();
        // The first step should always be a field.
        let first_step = lookup_iter.next()?;
        // This is good, since the first step into a LogEvent will also be a field.

        // This step largely exists so that we can make `cursor` a `Value` right off the bat.
        // We couldn't go like `let cursor = Value::from(self.fields)` since that'd take the value.
        let mut cursor = match first_step {
            Segment::Field(ref f) => {
                trace!(key = f, "Descending into map.");
                self.fields.get(*f)
            }
            // In this case, the user has passed us an invariant.
            Segment::Index(_) => {
                error!(
                    "Lookups into LogEvents should never start with indexes.\
                        Please report your config."
                );
                return None;
            }
        };

        for segment in lookup_iter {
            // Don't do extra work.
            if cursor.is_none() {
                break;
            }
            cursor = match (segment, cursor) {
                // Fields access maps.
                (Segment::Field(ref f), Some(Value::Map(map))) => {
                    trace!(key = %f, "Descending into map.");
                    map.get(*f)
                }
                // Indexes access arrays.
                (Segment::Index(i), Some(Value::Array(array))) => {
                    trace!(key = %i, "Descending into array.");
                    array.get(*i)
                }
                // The rest, it's not good.
                (Segment::Index(_), _) | (Segment::Field(_), _) => {
                    trace!("Unmatched lookup.");
                    None
                }
            }
        }

        // By the time we get here we either have the item, or nothing. Either case, we're correct.
        cursor
    }

    /// Get a mutable borrow of the value by lookup.
    #[instrument(level = "trace", skip(self))]
    pub fn get_mut<'a>(&mut self, lookup: impl Into<Lookup<'a>> + Debug) -> Option<&mut Value> {
        let lookup = lookup.into();
        let mut lookup_iter = lookup.iter();
        // The first step should always be a field.
        let first_step = lookup_iter.next()?;
        // This is good, since the first step into a LogEvent will also be a field.

        // This step largely exists so that we can make `cursor` a `Value` right off the bat.
        // We couldn't go like `let cursor = Value::from(self.fields)` since that'd take the value.
        let mut cursor = match first_step {
            Segment::Field(f) => {
                trace!(key = %f, "Descending into array.");
                self.fields.get_mut(*f)
            }
            // In this case, the user has passed us an invariant.
            Segment::Index(_) => {
                error!(
                    "Lookups into LogEvents should never start with indexes.\
                        Please report your config."
                );
                return None;
            }
        };

        for segment in lookup_iter {
            // Don't do extra work.
            if cursor.is_none() {
                break;
            }
            cursor = match (segment, cursor) {
                // Fields access maps.
                (Segment::Field(f), Some(Value::Map(map))) => {
                    trace!(key = %f, "Descending into map.");
                    map.get_mut(*f)
                }
                // Indexes access arrays.
                (Segment::Index(i), Some(Value::Array(array))) => {
                    trace!(key = %i, "Descending into array.");
                    array.get_mut(*i)
                }
                // The rest, it's not good.
                (Segment::Index(_), _) | (Segment::Field(_), _) => {
                    trace!("Unmatched lookup.");
                    None
                }
            }
        }

        // By the time we get here we either have the item, or nothing. Either case, we're correct.
        cursor
    }

    /// Determine if the log event contains a value at a given lookup.
    #[instrument(level = "trace", skip(self))]
    pub fn contains<'a>(&self, lookup: impl Into<Lookup<'a>> + Debug) -> bool {
        self.get(lookup).is_some()
    }

    /// Insert a value at a given lookup.
    #[instrument(level = "trace", skip(self))]
    pub fn insert(&mut self, lookup: LookupBuf, value: impl Into<Value> + Debug) -> Option<Value> {
        let mut seen_segments = vec![];
        let lookup_len = lookup.len();
        let mut lookup_iter = lookup.into_iter().enumerate();
        let mut value = value.into();
        // The first step should always be a field.
        let (_zero, first_step) = lookup_iter.next()?;
        seen_segments.push(first_step.clone());
        // This is good, since the first step into a LogEvent will also be a field.

        // This step largely exists so that we can make `cursor` a `Value` right off the bat.
        // We couldn't go like `let cursor = Value::from(self.fields)` since that'd take the value.
        let mut cursor = match first_step {
            SegmentBuf::Field(f) => {
                if lookup_len == 1 {
                    trace!(key = %f, value = ?value, "Inserted into root.");
                    return self.fields.insert(f, value);
                } else {
                    trace!(key = %f, "Descending into map.");
                    self.fields.entry(f.clone()).or_insert_with(|| {
                        trace!(key = %f, "Entry not found, inserting a null to build up.");
                        Value::Null
                    })
                }
            }
            // In this case, the user has passed us an invariant.
            SegmentBuf::Index(_) => {
                error!(
                    "Lookups into LogEvents should never start with indexes.\
                        Please report your config."
                );
                return None;
            }
        };

        let retval = None;

        for (index, segment) in lookup_iter {
            cursor = match (segment.clone(), cursor) {
                // Fields access maps.
                (SegmentBuf::Field(ref f), &mut Value::Map(ref mut map)) => {
                    if index == lookup_len - 1 {
                        // Terminus: We **must** insert here or abort.
                        trace!(key = %f, "Creating field inside map.");
                        return map.insert(f.clone(), value);
                    } else {
                        trace!(key = %f, "Descending into map.");
                        map.entry(f.clone()).or_insert_with(|| {
                            trace!(key = %f, "Entry not found, inserting a map to build up.");
                            Value::Map(Default::default())
                        })
                    }
                }
                // Indexes access arrays.
                (SegmentBuf::Index(i), &mut Value::Array(ref mut array)) => {
                    if index == lookup_len - 1 {
                        // Terminus: We **must** insert here or abort.
                        trace!(key = %i, "Terminus array index segment, inserting into index unconditionally.");
                        return match array.get_mut(i) {
                            None => {
                                trace!(key = %i, "Resizing array with Null values up to index, then pushing value.");
                                array.resize_with(i, || Value::Null);
                                core::mem::swap(array.index_mut(i), &mut value);
                                Some(Value::Null)
                            }
                            Some(target) => {
                                trace!(key = %i, "Swapping existing value at index for inserted value, returning it.");
                                let mut removed = Value::Null;
                                core::mem::swap(target, &mut removed);
                                Some(removed)
                            }
                        }
                    } else {
                        trace!(key = %i, "Descending into array.");
                        let len = array.len();
                        if len >= i {
                            array.get_mut(i).expect(&*format!("Array of length {} is expected to have value at index {}", len, i))
                        } else {
                            trace!(key = %i, "Descendent array was not long enough, resizing and pushing new value.");
                            array.resize_with(i.saturating_sub(1), || Value::Null);
                            array.push(value);
                            return Some(Value::Null);
                        }

                    }
                },
                (SegmentBuf::Field(_f), v ) if v == &mut Value::Null => {
                    trace!("Did not discover map to descend into, but found a `null`, presuming intent and inserting a map instead.");
                    let mut new = Value::Map(Default::default());
                    core::mem::swap(v, &mut new);
                    v
                },
                // The option of Index/Array was already caught. This is an error path but we can't fail.
                (SegmentBuf::Index(i), v) if v == &mut Value::Null => {
                    trace!("Did not discover map to descend into, but found a `null`, presuming intent and inserting an array instead.");
                    let mut array = Vec::with_capacity(i.saturating_add(1));
                    array.resize_with(i.saturating_add(1),|| Value::Null);
                    let mut new = Value::Array(array);
                    core::mem::swap(v, &mut new);
                    v
                },
                (_segment, _v) => {
                    debug!("Bailing on insert. There is an existing value which is not an array or map being inserted into.");
                    return None;
                },
            };
            seen_segments.push(segment.clone())
        }

        retval
    }

    /// Remove a value that exists at a given lookup.
    ///
    /// Setting `prune` to true will also remove the entries of maps and arrays that are emptied.
    #[instrument(level = "trace", skip(self))]
    pub fn remove<'a>(
        &mut self,
        lookup: impl Into<Lookup<'a>> + Debug,
        prune: bool,
    ) -> Option<Value> {
        let lookup = lookup.into();
        let lookup_len = lookup.len();
        let mut lookup_iter = lookup.iter().enumerate();
        // The first step should always be a field.
        let (_zero, first_step) = lookup_iter.next()?;
        // This is good, since the first step into a LogEvent will also be a field.

        // This step largely exists so that we can make `cursor` a `Value` right off the bat.
        // We couldn't go like `let cursor = Value::from(self.fields)` since that'd take the value.
        let mut cursor = match first_step {
            Segment::Field(f) => {
                if lookup_len == 1 {
                    trace!(key = %f, "Removed from root.");
                    return self.fields.remove(*f);
                } else {
                    trace!(key = %f, "Descending into map.");
                    self.fields.get_mut(*f)
                }
            }
            // In this case, the user has passed us an invariant.
            Segment::Index(_) => {
                error!(
                    "Lookups into LogEvents should never start with indexes.\
                        Please report your config."
                );
                return None;
            }
        };

        let mut retval = None;
        let mut needs_prune = None;
        for (index, segment) in lookup_iter {
            cursor = match (segment, cursor) {
                // Fields access maps.
                (Segment::Field(f), Some(Value::Map(map))) => {
                    if index == lookup_len {
                        trace!("Removing field inside map.");
                        retval = map.remove(*f);
                        if map.is_empty() && prune {
                            let mut cloned = lookup.clone();
                            cloned.pop();
                            needs_prune = Some(cloned);
                        }
                        break;
                    } else {
                        trace!(key = %f, "Descending into map.");
                        map.get_mut(*f)
                    }
                }
                // Indexes access arrays.
                (Segment::Index(i), Some(Value::Array(array))) => {
                    if index == lookup_len {
                        trace!("Removing index inside array.");
                        match array.get_mut(*i) {
                            None => None,
                            Some(target) => {
                                let mut removed = Value::Null;
                                core::mem::swap(target, &mut removed);
                                retval = Some(removed);
                                break;
                            }
                        }
                    } else {
                        trace!(key = %i, "Descending into array.");
                        array.get_mut(*i)
                    }
                }
                // The rest, it's not good.
                (Segment::Index(_), _) | (Segment::Field(_), _) => {
                    trace!("Unmatched lookup.");
                    None
                }
            }
        }

        if let Some(prune_here) = needs_prune {
            self.remove(prune_here, true);
        }

        retval
    }

    /// Iterate over the lookups available in this log event.
    ///
    /// This is notably different than the keys in a map, as this descends into things like arrays
    /// and maps. It also returns those array/map values during iteration.
    #[instrument(level = "trace", skip(self))]
    pub fn keys<'a>(&'a self) -> impl Iterator<Item = Lookup<'a>> + 'a {
        self.fields
            .iter()
            .map(|(k, v)| {
                let lookup = Lookup::from(k);
                trace!(prefix = %lookup, "Enqueuing for iteration.");
                let iter = Some(lookup.clone()).into_iter();
                let chain = v.lookups().map(move |l| {
                    let mut lookup = lookup.clone();
                    lookup.extend(l.clone());
                    lookup
                });
                iter.chain(chain)
            })
            .flatten()
    }

    /// Iterate over all lookup/value pairs.
    ///
    /// This is notably different than pairs in a map, as this descends into things like arrays and
    /// maps. It also returns those array/map values during iteration.
    #[instrument(level = "trace", skip(self))]
    pub fn all_fields<'a>(&'a self) -> impl Iterator<Item = (Lookup<'a>, &'a Value)> {
        self.fields
            .iter()
            .map(|(k, v)| {
                let lookup = Lookup::from(k);
                trace!(prefix = %lookup, "Enqueuing for iteration.");
                let iter = Some((lookup.clone(), v)).into_iter();
                let chain = v.pairs().map(move |(l, v)| {
                    let mut lookup = lookup.clone();
                    lookup.extend(l.clone());
                    (lookup, v)
                });
                iter.chain(chain)
            })
            .flatten()
    }

    /// Determine if the log event is empty of fields.
    #[instrument(level = "trace", skip(self))]
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Return an entry for the given lookup.
    #[instrument(level = "trace", skip(self, lookup), fields(lookup = %lookup), err)]
    fn entry(&mut self, lookup: LookupBuf) -> crate::Result<Entry<String, Value>> {
        trace!("Seeking to entry.");
        let mut walker = lookup.into_iter().enumerate();

        let mut current_pointer = if let Some((index, SegmentBuf::Field(segment))) = walker.next() {
            trace!(%segment, index, "Seeking segment.");
            self.fields.entry(segment)
        } else {
            unreachable!(
                "It is an invariant to have a `Lookup` without a contained `Segment`.\
                `Lookup::is_valid` should catch this during `Lookup` creation, maybe it was not \
                called?."
            );
        };

        for (index, segment) in walker {
            trace!(%segment, index, "Seeking next segment.");
            current_pointer = match (segment, current_pointer) {
                (SegmentBuf::Field(field), Entry::Occupied(entry)) => match entry.into_mut() {
                    Value::Map(map) => map.entry(field),
                    v => return Err(format!("Looking up field on a non-map value: {:?}", v).into()),
                },
                (SegmentBuf::Field(field), Entry::Vacant(entry)) => {
                    trace!(segment = %field, index, "Met vacant entry.");
                    return Err(format!(
                        "Tried to step into `{}` of `{}`, but it did not exist.",
                        field,
                        entry.key()
                    )
                    .into());
                }
                _ => return Err("The entry API cannot yet descend into array indices.".into()),
            };
        }
        trace!(entry = ?current_pointer, "Result.");
        Ok(current_pointer)
    }

    /// Returns the entire event as a `Value::Map`.
    #[instrument(level = "trace", skip(self))]
    pub fn take(self) -> Value {
        Value::Map(self.fields)
    }

    /// Get a borrow of the contained fields.
    #[instrument(level = "trace", skip(self))]
    pub fn inner(&mut self) -> &BTreeMap<String, Value> {
        &self.fields
    }

    /// Get a mutable borrow of the contained fields.
    #[instrument(level = "trace", skip(self))]
    pub fn inner_mut(&mut self) -> &mut BTreeMap<String, Value> {
        &mut self.fields
    }
}

impl From<BTreeMap<String, Value>> for LogEvent {
    fn from(map: BTreeMap<String, Value>) -> Self {
        LogEvent { fields: map }
    }
}

impl Into<BTreeMap<String, Value>> for LogEvent {
    fn into(self) -> BTreeMap<String, Value> {
        let Self { fields } = self;
        fields
    }
}

impl From<HashMap<String, Value>> for LogEvent {
    fn from(map: HashMap<String, Value>) -> Self {
        LogEvent {
            fields: map.into_iter().collect(),
        }
    }
}

impl Into<HashMap<String, Value>> for LogEvent {
    fn into(self) -> HashMap<String, Value> {
        self.fields.into_iter().collect()
    }
}

impl TryFrom<serde_json::Value> for LogEvent {
    type Error = crate::Error;

    fn try_from(map: serde_json::Value) -> Result<Self, Self::Error> {
        match map {
            serde_json::Value::Object(fields) => Ok(LogEvent::from(
                fields
                    .into_iter()
                    .map(|(k, v)| (k, v.into()))
                    .collect::<BTreeMap<_, _>>(),
            )),
            _ => Err(crate::Error::from(
                "Attempted to convert non-Object JSON into a LogEvent.",
            )),
        }
    }
}

impl TryInto<serde_json::Value> for LogEvent {
    type Error = crate::Error;

    fn try_into(self) -> Result<serde_json::Value, Self::Error> {
        let Self { fields } = self;
        Ok(serde_json::to_value(fields)?)
    }
}

impl<'a, T> std::ops::Index<T> for LogEvent
where
    T: Into<Lookup<'a>> + Debug,
{
    type Output = Value;

    fn index(&self, key: T) -> &Value {
        self.get(key).expect("Key not found.")
    }
}

impl<'a, T> std::ops::IndexMut<T> for LogEvent
where
    T: Into<Lookup<'a>> + Debug,
{
    fn index_mut(&mut self, key: T) -> &mut Value {
        self.get_mut(key).expect("Key not found.")
    }
}

impl<'a, V> Extend<(LookupBuf, V)> for LogEvent
where
    V: Into<Value>,
{
    fn extend<I: IntoIterator<Item = (LookupBuf, V)>>(&mut self, iter: I) {
        for (k, v) in iter {
            self.insert(k, v.into());
        }
    }
}

// Allow converting any kind of appropriate key/value iterator directly into a LogEvent.
impl<'a, V: Into<Value>> FromIterator<(LookupBuf, V)> for LogEvent {
    fn from_iter<T: IntoIterator<Item = (LookupBuf, V)>>(iter: T) -> Self {
        let mut log_event = LogEvent::default();
        log_event.extend(iter);
        log_event
    }
}

/// Converts event into an iterator over top-level key/value pairs.
impl IntoIterator for LogEvent {
    type Item = (String, Value);
    type IntoIter = std::collections::btree_map::IntoIter<String, Value>;

    fn into_iter(self) -> Self::IntoIter {
        self.fields.into_iter()
    }
}

impl Serialize for LogEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_map(self.fields.iter())
    }
}

impl<'de> Deserialize<'de> for LogEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(crate::event::util::LogEventVisitor)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_util::open_fixture;
    use serde_json::json;
    use tracing::trace;

    mod insert {
        use super::*;

        #[test]
        fn root() -> crate::Result<()> {
            crate::test_util::trace_init();
            let mut event = LogEvent::default();
            let lookup= LookupBuf::from_str("root")?;
            let value = Value::Null;
            event.insert(lookup, value.clone());
            assert_eq!(event.inner()["root"], value);
            Ok(())
        }

        #[test]
        fn map_field() -> crate::Result<()> {
            crate::test_util::trace_init();
            let mut event = LogEvent::default();
            let lookup= LookupBuf::from_str("root.field")?;
            let value = Value::Null;
            event.insert(lookup, value.clone());
            assert_eq!(event.inner()["root"].as_map()["field"], value);
            Ok(())
        }

        #[test]
        fn nested_map_field() -> crate::Result<()> {
            crate::test_util::trace_init();
            let mut event = LogEvent::default();
            let lookup= LookupBuf::from_str("root.field.subfield")?;
            let value = Value::Null;
            event.insert(lookup, value.clone());
            assert_eq!(event.inner()["root"].as_map()["field"].as_map()["subfield"], value);
            Ok(())
        }

        #[test]
        fn array_field() -> crate::Result<()> {
            crate::test_util::trace_init();
            let mut event = LogEvent::default();
            let lookup= LookupBuf::from_str("root[0]")?;
            let value = Value::Null;
            event.insert(lookup, value.clone());
            assert_eq!(event.inner()["root"].as_array()[0], value);
            Ok(())
        }

        #[test]
        fn array_field_nested_array() -> crate::Result<()> {
            crate::test_util::trace_init();
            let mut event = LogEvent::default();
            let lookup= LookupBuf::from_str("root[0][0]")?;
            let value = Value::Null;
            event.insert(lookup, value.clone());
            assert_eq!(event.inner()["root"].as_array()[0], value);
            Ok(())
        }

        #[test]
        fn array_field_nested_map() -> crate::Result<()> {
            crate::test_util::trace_init();
            let mut event = LogEvent::default();
            let lookup= LookupBuf::from_str("root[0].nested")?;
            let value = Value::Null;
            event.insert(lookup, value.clone());
            assert_eq!(event.inner()["root"].as_array()[0].as_map()["nested"], value);
            Ok(())
        }
    }

    // This test iterates over the `tests/data/fixtures/log_event` folder and:
    //   * Ensures the EventLog parsed from bytes and turned into a serde_json::Value are equal to the
    //     item being just plain parsed as json.
    //
    // Basically: This test makes sure we aren't mutilating any content users might be sending.
    #[test]
    fn json_value_to_vector_log_event_to_json_value() {
        crate::test_util::trace_init();
        const FIXTURE_ROOT: &str = "tests/data/fixtures/log_event";

        trace!(?FIXTURE_ROOT, "Opening.");
        std::fs::read_dir(FIXTURE_ROOT)
            .unwrap()
            .for_each(|fixture_file| match fixture_file {
                Ok(fixture_file) => {
                    let path = fixture_file.path();
                    tracing::trace!(?path, "Opening.");
                    let serde_value = open_fixture(&path).unwrap();

                    let vector_value = LogEvent::try_from(serde_value.clone()).unwrap();
                    let serde_value_again: serde_json::Value =
                        vector_value.clone().try_into().unwrap();

                    tracing::trace!(
                        ?path,
                        ?serde_value,
                        ?vector_value,
                        ?serde_value_again,
                        "Asserting equal."
                    );
                    assert_eq!(serde_value, serde_value_again);
                }
                _ => panic!("This test should never read Err'ing test fixtures."),
            });
    }

    // We use `serde_json` pointers in this test to ensure we're validating that Vector correctly inputs and outputs things as expected.
    #[test]
    fn entry() {
        crate::test_util::trace_init();
        let fixture =
            open_fixture("tests/data/fixtures/log_event/motivatingly-complex.json").unwrap();
        let mut event = LogEvent::try_from(fixture).unwrap();

        let lookup = LookupBuf::from_str("non-existing").unwrap();
        let entry = event.entry(lookup).unwrap();
        let fallback = json!(
            "If you don't see this, the `LogEvent::entry` API is not working on non-existing lookups."
        );
        entry.or_insert_with(|| fallback.clone().into());
        let json: serde_json::Value = event.clone().try_into().unwrap();
        trace!(?json);
        assert_eq!(json.pointer("/non-existing"), Some(&fallback));

        let lookup = LookupBuf::from_str("nulled").unwrap();
        let entry = event.entry(lookup).unwrap();
        let fallback = json!(
            "If you see this, the `LogEvent::entry` API is not working on existing, single segment lookups."
        );
        entry.or_insert_with(|| fallback.clone().into());
        let json: serde_json::Value = event.clone().try_into().unwrap();
        assert_eq!(json.pointer("/nulled"), Some(&serde_json::Value::Null));

        let lookup = LookupBuf::from_str("map.basic").unwrap();
        let entry = event.entry(lookup).unwrap();
        let fallback = json!(
            "If you see this, the `LogEvent::entry` API is not working on existing, double segment lookups."
        );
        entry.or_insert_with(|| fallback.clone().into());
        let json: serde_json::Value = event.clone().try_into().unwrap();
        assert_eq!(
            json.pointer("/map/basic"),
            Some(&serde_json::Value::Bool(true))
        );

        let lookup = LookupBuf::from_str("map.map.buddy").unwrap();
        let entry = event.entry(lookup).unwrap();
        let fallback = json!(
            "If you see this, the `LogEvent::entry` API is not working on existing, multi-segment lookups."
        );
        entry.or_insert_with(|| fallback.clone().into());
        let json: serde_json::Value = event.clone().try_into().unwrap();
        assert_eq!(
            json.pointer("/map/map/buddy"),
            Some(&serde_json::Value::Number((-1).into()))
        );

        let lookup = LookupBuf::from_str("map.map.non-existing").unwrap();
        let entry = event.entry(lookup).unwrap();
        let fallback = json!(
            "If you don't see this, the `LogEvent::entry` API is not working on non-existing multi-segment lookups."
        );
        entry.or_insert_with(|| fallback.clone().into());
        let json: serde_json::Value = event.clone().try_into().unwrap();
        assert_eq!(json.pointer("/map/map/non-existing"), Some(&fallback));
    }
}
