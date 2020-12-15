#![allow(clippy::needless_collect)]
use crate::event::{
    lookup::{Segment, SegmentBuf},
    Lookup, LookupBuf, Value,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{btree_map::Entry, BTreeMap, HashMap},
    convert::{TryFrom, TryInto},
    fmt::Debug,
    iter::FromIterator,
};

#[derive(PartialEq, Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogEvent {
    #[serde(flatten)]
    fields: BTreeMap<String, Value>,
}

impl LogEvent {
    /// Get an immutable borrow of the given value by lookup.
    #[instrument(level = "trace", skip(self))]
    pub fn get<'a>(&self, lookup: impl Into<Lookup<'a>> + Debug) -> Option<&Value> {
        let mut working_lookup = lookup.into();
        // The first step should always be a field.
        let this_segment = working_lookup.pop_front().unwrap();
        // This is good, since the first step into a LogEvent will also be a field.

        // This step largely exists so that we can make `cursor` a `Value` right off the bat.
        // We couldn't go like `let cursor = Value::from(self.fields)` since that'd take the value.
        match this_segment {
            Segment::Coalesce(v) => unimplemented!(),
            Segment::Field {
                name,
                requires_quoting: _,
            } => {
                if working_lookup.len() == 0 {
                    // Terminus: We **must** insert here or abort.
                    trace!(key = ?name, "Getting from root.");
                    self.fields.get(name)
                } else {
                    trace!(key = ?name, "Descending into map.");
                    match self.fields.get(name) {
                        Some(v) => v.get(working_lookup).ok().unwrap_or(None),
                        None => None,
                    }
                }
            },
            // In this case, the user has passed us an invariant.
            Segment::Index(_) => {
                error!(
                    "Lookups into LogEvents should never start with indexes.\
                        Please report your config."
                );
                None
            },
        }
    }

    /// Get a mutable borrow of the value by lookup.
    #[instrument(level = "trace", skip(self))]
    pub fn get_mut<'a>(&mut self, lookup: impl Into<Lookup<'a>> + Debug) -> Option<&mut Value> {
        let mut working_lookup = lookup.into();
        // The first step should always be a field.
        let this_segment = working_lookup.pop_front().unwrap();
        // This is good, since the first step into a LogEvent will also be a field.

        // This step largely exists so that we can make `cursor` a `Value` right off the bat.
        // We couldn't go like `let cursor = Value::from(self.fields)` since that'd take the value.
        match this_segment {
            Segment::Coalesce(v) => unimplemented!(),
            Segment::Field {
                name,
                requires_quoting: _,
            } => {
                if working_lookup.len() == 0 {
                    // Terminus: We **must** insert here or abort.
                    trace!(key = ?name, "Getting from root.");
                    self.fields.get_mut(name)
                } else {
                    trace!(key = ?name, "Descending into map.");
                    match self.fields.get_mut(name) {
                        Some(v) => v.get_mut(working_lookup).ok().unwrap_or(None),
                        None => None,
                    }
                }
            },
            // In this case, the user has passed us an invariant.
            Segment::Index(_) => {
                error!(
                    "Lookups into LogEvents should never start with indexes.\
                        Please report your config."
                );
                None
            },
        }
    }

    /// Determine if the log event contains a value at a given lookup.
    #[instrument(level = "trace", skip(self))]
    pub fn contains<'a>(&self, lookup: impl Into<Lookup<'a>> + Debug) -> bool {
        self.get(lookup).is_some()
    }

    /// Insert a value at a given lookup.
    #[instrument(level = "trace", skip(self))]
    pub fn insert(&mut self, lookup: LookupBuf, value: impl Into<Value> + Debug) -> Option<Value> {
        let mut working_lookup = lookup;
        // The first step should always be a field.
        let this_segment = working_lookup.pop_front().unwrap();
        // This is good, since the first step into a LogEvent will also be a field.

        // This step largely exists so that we can make `cursor` a `Value` right off the bat.
        // We couldn't go like `let cursor = Value::from(self.fields)` since that'd take the value.
        match this_segment {
            SegmentBuf::Coalesce(v) => unimplemented!(),
            SegmentBuf::Field {
                name,
                requires_quoting: _,
            } => {
                let next_value = match working_lookup.get(0) {
                    Some(SegmentBuf::Index(next_len)) => Value::Array(Vec::with_capacity(*next_len)),
                    Some(SegmentBuf::Field { .. }) => Value::Map(Default::default()),
                    Some(SegmentBuf::Coalesce(set)) => {
                        let mut cursor_set = set;
                        loop {
                            match cursor_set.get(0).and_then(|v| v.get(0)) {
                                None => return None,
                                Some(SegmentBuf::Field { .. }) => break Value::Map(Default::default()),
                                Some(SegmentBuf::Index(i)) => break Value::Array(Vec::with_capacity(*i)),
                                Some(SegmentBuf::Coalesce(set)) => cursor_set = &set,
                            }
                        }
                    }
                    None => {
                        trace!(key = ?name, "Getting from root.");
                        return self.fields.insert(name, value.into())
                    }
                };
                self.fields.entry(name)
                    .or_insert_with(|| {
                        trace!("Inserting at leaf.");
                        next_value
                    })
                    .insert(working_lookup, value).ok().unwrap_or(None)
            },
            // In this case, the user has passed us an invariant.
            SegmentBuf::Index(_) => {
                error!(
                    "Lookups into LogEvents should never start with indexes.\
                        Please report your config."
                );
                None
            },
        }
    }

    /// Remove a value that exists at a given lookup.
    ///
    /// Setting `prune` to true will also remove the entries of maps and arrays that are emptied.
    #[instrument(level = "trace", skip(self))]
    pub fn remove<'lookup>(
        &mut self,
        lookup: impl Into<Lookup<'lookup>> + Debug,
        prune: bool,
    ) -> Option<Value> {
        let mut working_lookup = lookup.into();
        // The first step should always be a field.
        let this_segment = working_lookup.pop_front().unwrap();
        // This step largely exists so that we can make `cursor` a `Value` right off the bat.
        // We couldn't go like `let cursor = Value::from(self.fields)` since that'd take the value.
        match this_segment {
            Segment::Coalesce(v) => unimplemented!(),
            Segment::Field {
                name,
                requires_quoting: _,
            } => {
                if working_lookup.len() == 0 {
                    // Terminus: We **must** insert here or abort.
                    trace!(key = ?name, "Getting from root.");
                    let retval = self.fields.remove(name);
                    if prune && self.fields.get(name) == Some(&Value::Null) {
                        self.fields.remove(name);
                    }
                    retval

                } else {
                    trace!(key = ?name, "Descending into map.");
                    let retval = match self.fields.get_mut(name) {
                        Some(v) => v.remove(working_lookup, prune).ok().unwrap_or(None),
                        None => None,
                    };
                    if prune && self.fields.get(name) == Some(&Value::Null) {
                        self.fields.remove(name);
                    }
                    retval

                }
            },
            // In this case, the user has passed us an invariant.
            Segment::Index(_) => {
                error!(
                    "Lookups into LogEvents should never start with indexes.\
                        Please report your config."
                );
                None
            },
        }
    }

    /// Iterate over the lookups available in this log event.
    ///
    /// This is notably different than the keys in a map, as this descends into things like arrays
    /// and maps. It also returns those array/map values during iteration.
    #[instrument(level = "trace", skip(self))]
    pub fn keys<'a>(&'a self, only_leaves: bool) -> impl Iterator<Item = Lookup<'a>> + 'a {
        self.fields
            .iter()
            .map(move |(k, v)| {
                let lookup = Lookup::from(k);
                v.lookups(Some(lookup), only_leaves)
            })
            .flatten()
    }

    /// Iterate over all lookup/value pairs.
    ///
    /// This is notably different than pairs in a map, as this descends into things like arrays and
    /// maps. It also returns those array/map values during iteration.
    #[instrument(level = "trace", skip(self))]
    pub fn pairs<'a>(&'a self, only_leaves: bool) -> impl Iterator<Item = (Lookup<'a>, &'a Value)> {
        self.fields
            .iter()
            .map(move |(k, v)| {
                let lookup = Lookup::from(k);
                v.pairs(Some(lookup), only_leaves)
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

        let mut current_pointer = if let Some((
            index,
            SegmentBuf::Field {
                name: segment,
                requires_quoting: _,
            },
        )) = walker.next()
        {
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
                (
                    SegmentBuf::Field {
                        name,
                        requires_quoting: _,
                    },
                    Entry::Occupied(entry),
                ) => match entry.into_mut() {
                    Value::Map(map) => map.entry(name),
                    v => return Err(format!("Looking up field on a non-map value: {:?}", v).into()),
                },
                (
                    SegmentBuf::Field {
                        name,
                        requires_quoting: _,
                    },
                    Entry::Vacant(entry),
                ) => {
                    trace!(segment = %name, index, "Met vacant entry.");
                    return Err(format!(
                        "Tried to step into `{}` of `{}`, but it did not exist.",
                        name,
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


impl<T> std::ops::Index<T> for LogEvent
    where
        T: Into<Lookup<'static>> + Debug,
{
    type Output = Value;

    fn index(&self, key: T) -> &Value {
        self.get(key).expect("Key not found.")
    }
}

impl<T> std::ops::IndexMut<T> for LogEvent
    where
        T: Into<Lookup<'static>> + Debug,
{
    fn index_mut(&mut self, key: T) -> &mut Value {
        self.get_mut(key).expect("Key not found.")
    }
}


#[cfg(test)]
mod test {
    use super::*;
    use crate::test_util::open_fixture;
    use serde_json::json;
    use tracing::trace;

    mod insert_get_remove {
        use super::*;

        #[test]
        fn root() -> crate::Result<()> {
            crate::test_util::trace_init();
            let mut event = LogEvent::default();
            let lookup = LookupBuf::from_str("root")?;
            let mut value = Value::Boolean(true);
            event.insert(lookup.clone(), value.clone());
            assert_eq!(event.inner()["root"], value);
            assert_eq!(event.get(&lookup), Some(&value));
            assert_eq!(event.get_mut(&lookup), Some(&mut value));
            assert_eq!(event.remove(&lookup, false), Some(value));
            Ok(())
        }

        #[test]
        fn quoted_from_str() -> crate::Result<()> {
            // In this test, we make sure the quotes are stripped, since it's a parsed lookup.
            crate::test_util::trace_init();
            let mut event = LogEvent::default();
            let lookup = LookupBuf::from_str("root.\"doot\"")?;
            let mut value = Value::Boolean(true);
            event.insert(lookup.clone(), value.clone());
            assert_eq!(event.inner()["root"].as_map()["doot"], value);
            assert_eq!(event.get(&lookup), Some(&value));
            assert_eq!(event.get_mut(&lookup), Some(&mut value));
            assert_eq!(event.remove(&lookup, false), Some(value));
            Ok(())
        }

        #[test]
        fn root_with_buddy() -> crate::Result<()> {
            crate::test_util::trace_init();
            let mut event = LogEvent::default();
            let lookup = LookupBuf::from_str("root")?;
            let mut value = Value::Boolean(true);
            event.insert(lookup.clone(), value.clone());
            assert_eq!(event.inner()["root"], value);
            assert_eq!(event.get(&lookup), Some(&value));
            assert_eq!(event.get_mut(&lookup), Some(&mut value));
            assert_eq!(event.remove(&lookup, false), Some(value));

            let lookup = LookupBuf::from_str("scrubby")?;
            let mut value = Value::Boolean(true);
            event.insert(lookup.clone(), value.clone());
            assert_eq!(event.inner()["scrubby"], value);
            assert_eq!(event.get(&lookup), Some(&value));
            assert_eq!(event.get_mut(&lookup), Some(&mut value));
            assert_eq!(event.remove(&lookup, false), Some(value));
            Ok(())
        }

        #[test]
        fn coalesced_root() -> crate::Result<()> {
            crate::test_util::trace_init();
            let mut event = LogEvent::default();
            let lookup = LookupBuf::from_str("(snoot | boot).loot")?;
            let mut value = Value::Boolean(true);
            event.insert(lookup.clone(), value.clone());
            assert_eq!(event.inner()["snoot"].as_map()["loot"], value);
            assert_eq!(event.get(&lookup), Some(&value));
            assert_eq!(event.get_mut(&lookup), Some(&mut value));
            assert_eq!(event.remove(&lookup, false), Some(value));

            let lookup = LookupBuf::from_str("boot")?;
            assert_eq!(event.get(&lookup), None);

            Ok(())
        }

        #[test]
        fn coalesced_nested() -> crate::Result<()> {
            crate::test_util::trace_init();
            let mut event = LogEvent::default();
            let lookup = LookupBuf::from_str("root.(snoot | boot)")?;
            let mut value = Value::Boolean(true);
            event.insert(lookup.clone(), value.clone());
            assert_eq!(event.inner()["root"].as_map()["snoot"], value);
            assert_eq!(event.get(&lookup), Some(&value));
            assert_eq!(event.get_mut(&lookup), Some(&mut value));
            assert_eq!(event.remove(&lookup, false), Some(value));

            let lookup = LookupBuf::from_str("root.boot")?;
            assert_eq!(event.get(&lookup), None);

            Ok(())
        }

        #[test]
        fn coalesced_with_nesting() -> crate::Result<()> {
            crate::test_util::trace_init();
            let mut event = LogEvent::default();
            let lookup = LookupBuf::from_str("root.(snoot | boot.beep).leep")?;
            let mut value = Value::Boolean(true);

            // This is deliberately duplicated!!! Because it's a coalesce both fields will be filled.
            // This is the point of the test!
            event.insert(lookup.clone(), value.clone());
            event.insert(lookup.clone(), value.clone());

            assert_eq!(event.inner()["root"].as_map()["snoot"].as_map()["leep"], value);
            assert_eq!(event.inner()["root"].as_map()["boot"].as_map()["beep"].as_map()["leep"], value);

            // This repeats, because it's the purpose of the test!
            assert_eq!(event.get(&lookup), Some(&value));
            assert_eq!(event.get_mut(&lookup), Some(&mut value));
            assert_eq!(event.remove(&lookup, false), Some(value.clone()));
            // Now that we removed one, we will get the other.
            assert_eq!(event.get(&lookup), Some(&value));
            assert_eq!(event.get_mut(&lookup), Some(&mut value));
            assert_eq!(event.remove(&lookup, false), Some(value.clone()));

            Ok(())
        }
        #[test]
        fn map_field() -> crate::Result<()> {
            crate::test_util::trace_init();
            let mut event = LogEvent::default();
            let lookup = LookupBuf::from_str("root.field")?;
            let mut value = Value::Boolean(true);
            event.insert(lookup.clone(), value.clone());
            assert_eq!(event.inner()["root"].as_map()["field"], value);
            assert_eq!(event.get(&lookup), Some(&value));
            assert_eq!(event.get_mut(&lookup), Some(&mut value));
            assert_eq!(event.remove(&lookup, false), Some(value));
            Ok(())
        }

        #[test]
        fn nested_map_field() -> crate::Result<()> {
            crate::test_util::trace_init();
            let mut event = LogEvent::default();
            let lookup = LookupBuf::from_str("root.field.subfield")?;
            let mut value = Value::Boolean(true);
            event.insert(lookup.clone(), value.clone());
            assert_eq!(
                event.inner()["root"].as_map()["field"].as_map()["subfield"],
                value
            );
            assert_eq!(event.get(&lookup), Some(&value));
            assert_eq!(event.get_mut(&lookup), Some(&mut value));
            assert_eq!(event.remove(&lookup, false), Some(value));
            Ok(())
        }

        #[test]
        fn array_field() -> crate::Result<()> {
            crate::test_util::trace_init();
            let mut event = LogEvent::default();
            let lookup = LookupBuf::from_str("root[0]")?;
            let mut value = Value::Boolean(true);
            event.insert(lookup.clone(), value.clone());
            assert_eq!(event.inner()["root"].as_array()[0], value);
            assert_eq!(event.get(&lookup), Some(&value));
            assert_eq!(event.get_mut(&lookup), Some(&mut value));
            assert_eq!(event.remove(&lookup, false), Some(value));
            Ok(())
        }

        #[test]
        fn array_reverse_population() -> crate::Result<()> {
            crate::test_util::trace_init();
            let mut event = LogEvent::default();
            let lookup = LookupBuf::from_str("root[2]")?;
            let mut value = Value::Boolean(true);
            event.insert(lookup.clone(), value.clone());
            assert_eq!(event.inner()["root"].as_array()[2], value);
            assert_eq!(event.get(&lookup), Some(&value));
            assert_eq!(event.get_mut(&lookup), Some(&mut value));
            assert_eq!(event.remove(&lookup, false), Some(value));

            let lookup = LookupBuf::from_str("root[1]")?;
            let mut value = Value::Boolean(true);
            event.insert(lookup.clone(), value.clone());
            assert_eq!(event.inner()["root"].as_array()[1], value);
            assert_eq!(event.get(&lookup), Some(&value));
            assert_eq!(event.get_mut(&lookup), Some(&mut value));
            assert_eq!(event.remove(&lookup, false), Some(value));

            let lookup = LookupBuf::from_str("root[0]")?;
            let mut value = Value::Boolean(true);
            event.insert(lookup.clone(), value.clone());
            assert_eq!(event.inner()["root"].as_array()[0], value);
            assert_eq!(event.get(&lookup), Some(&value));
            assert_eq!(event.get_mut(&lookup), Some(&mut value));
            assert_eq!(event.remove(&lookup, false), Some(value));
            Ok(())
        }

        #[test]
        fn array_field_nested_array() -> crate::Result<()> {
            crate::test_util::trace_init();
            let mut event = LogEvent::default();
            let lookup = LookupBuf::from_str("root[0][0]")?;
            let mut value = Value::Boolean(true);
            event.insert(lookup.clone(), value.clone());
            assert_eq!(event.inner()["root"].as_array()[0].as_array()[0], value);
            assert_eq!(event.get(&lookup), Some(&value));
            assert_eq!(event.get_mut(&lookup), Some(&mut value));
            assert_eq!(event.remove(&lookup, false), Some(value));
            Ok(())
        }

        #[test]
        fn array_field_nested_map() -> crate::Result<()> {
            crate::test_util::trace_init();
            let mut event = LogEvent::default();
            let lookup = LookupBuf::from_str("root[0].nested")?;
            let mut value = Value::Boolean(true);
            event.insert(lookup.clone(), value.clone());
            assert_eq!(
                event.inner()["root"].as_array()[0].as_map()["nested"],
                value
            );
            assert_eq!(event.get(&lookup), Some(&value));
            assert_eq!(event.get_mut(&lookup), Some(&mut value));
            assert_eq!(event.remove(&lookup, false), Some(value));
            Ok(())
        }

        #[test]
        fn perverse() -> crate::Result<()> {
            crate::test_util::trace_init();
            let mut event = LogEvent::default();
            let lookup = LookupBuf::from_str(
                "root[10].nested[10].more[9].than[8].there[7][6][5].we.go.friends.look.at.this",
            )?;
            let mut value = Value::Boolean(true);
            event.insert(lookup.clone(), value.clone());
            assert_eq!(
                event.inner()["root"].as_array()[10].as_map()["nested"].as_array()[10].as_map()
                    ["more"]
                    .as_array()[9]
                    .as_map()["than"]
                    .as_array()[8]
                    .as_map()["there"]
                    .as_array()[7]
                    .as_array()[6]
                    .as_array()[5]
                    .as_map()["we"]
                    .as_map()["go"]
                    .as_map()["friends"]
                    .as_map()["look"]
                    .as_map()["at"]
                    .as_map()["this"],
                value
            );
            assert_eq!(event.get(&lookup), Some(&value));
            assert_eq!(event.get_mut(&lookup), Some(&mut value));
            assert_eq!(event.remove(&lookup, false), Some(value));
            Ok(())
        }
    }

    mod corner_cases {
        use super::*;

        // While authors should prefer to set an array via `event.insert(lookup_to_array, array)`,
        // there are some cases where we want to insert 1 by one. Make sure this can happen.
        #[test]
        fn iteratively_populate_array() -> crate::Result<()> {
            crate::test_util::trace_init();
            let mut event = LogEvent::default();
            let lookups = vec![
                LookupBuf::from_str("root.nested[0]")?,
                LookupBuf::from_str("root.nested[1]")?,
                LookupBuf::from_str("root.nested[2]")?,
                LookupBuf::from_str("other[1][0]")?,
                LookupBuf::from_str("other[1][1].a")?,
                LookupBuf::from_str("other[1][1].b")?,
            ];
            let value = Value::Boolean(true);
            for lookup in lookups.clone() {
                event.insert(lookup, value.clone());
            }
            let pairs = event.keys(true).collect::<Vec<_>>();
            for lookup in lookups {
                assert!(
                    pairs.contains(&lookup.clone_lookup()),
                    "Failed while looking for {}",
                    lookup
                );
            }
            Ok(())
        }

        // While authors should prefer to set an array via `event.insert(lookup_to_array, array)`,
        // there are some cases where we want to insert 1 by one. Make sure this can happen.
        #[test]
        fn iteratively_populate_array_reverse() -> crate::Result<()> {
            crate::test_util::trace_init();
            let mut event = LogEvent::default();
            let lookups = vec![
                LookupBuf::from_str("root.nested[1]")?,
                LookupBuf::from_str("root.nested[0]")?,
                LookupBuf::from_str("other[1][1]")?,
                LookupBuf::from_str("other[0][1].a")?,
            ];
            let value = Value::Boolean(true);
            for lookup in lookups.clone() {
                event.insert(lookup, value.clone());
            }
            let pairs = event.keys(false).collect::<Vec<_>>();
            for lookup in lookups.clone() {
                assert!(
                    pairs.contains(&lookup.clone_lookup()),
                    "Failed while looking for {:?} in {:?}",
                    lookup,
                    pairs
                );
            }
            Ok(())
        }

        // While authors should prefer to set an map via `event.insert(lookup_to_map, map)`,
        // there are some cases where we want to insert 1 by one. Make sure this can happen.
        #[test]
        fn iteratively_populate_map() -> crate::Result<()> {
            crate::test_util::trace_init();
            let mut event = LogEvent::default();
            let lookups = vec![
                LookupBuf::from_str("root.one")?,
                LookupBuf::from_str("root.two")?,
                LookupBuf::from_str("root.three.a")?,
                LookupBuf::from_str("root.three.b")?,
                LookupBuf::from_str("root.three.c")?,
                LookupBuf::from_str("root.four[0]")?,
                LookupBuf::from_str("root.four[1]")?,
                LookupBuf::from_str("root.four[2]")?,
            ];
            let value = Value::Boolean(true);
            for lookup in lookups.clone() {
                event.insert(lookup, value.clone());
            }
            // Note: Two Lookups are only the same if the string slices underneath are too.
            //       LookupBufs this rule does not apply.
            let pairs = event.keys(true).map(|k| k.into_buf()).collect::<Vec<_>>();
            for lookup in lookups {
                assert!(
                    pairs.contains(&lookup),
                    "Failed while looking for {}",
                    lookup
                );
            }
            Ok(())
        }
    }

    #[test]
    fn keys_and_pairs() -> crate::Result<()> {
        crate::test_util::trace_init();

        let mut event = LogEvent::default();
        // We opt for very small arrays here to avoid having to iterate a bunch.
        let lookup = LookupBuf::from_str("snooper.booper[1][2]")?;
        event.insert(lookup, Value::Null);
        let lookup = LookupBuf::from_str("whomp[1].glomp[1]")?;
        event.insert(lookup, Value::Null);
        let lookup = LookupBuf::from_str("zoop")?;
        event.insert(lookup, Value::Null);

        // Collect and sort since we don't want a flaky test on iteration do we?
        let mut keys = event.keys(false).collect::<Vec<_>>();
        keys.sort();
        let mut pairs = event.pairs(false).collect::<Vec<_>>();
        pairs.sort_by(|v, x| v.0.cmp(&x.0));

        // Ensure a new field element that was injected is iterated over.
        let expected = Lookup::from_str("snooper").unwrap();
        assert_eq!(keys[0], expected);
        assert_eq!(pairs[0].0, expected);
        let expected = Lookup::from_str("snooper.booper").unwrap();
        assert_eq!(keys[1], expected);
        assert_eq!(pairs[1].0, expected);
        // Ensure a new array element that was injected is iterated over.
        let expected = Lookup::from_str("snooper.booper[0]").unwrap();
        assert_eq!(keys[2], expected);
        assert_eq!(pairs[2].0, expected);
        let expected = Lookup::from_str("snooper.booper[1]").unwrap();
        assert_eq!(keys[3], expected);
        assert_eq!(pairs[3].0, expected);
        let expected = Lookup::from_str("snooper.booper[1][0]").unwrap();
        assert_eq!(keys[4], expected);
        assert_eq!(pairs[4].0, expected);
        let expected = Lookup::from_str("snooper.booper[1][1]").unwrap();
        assert_eq!(keys[5], expected);
        assert_eq!(pairs[5].0, expected);
        let expected = Lookup::from_str("snooper.booper[1][2]").unwrap();
        assert_eq!(keys[6], expected);
        assert_eq!(pairs[6].0, expected);
        // Try inside arrays now.
        let expected = Lookup::from_str("whomp").unwrap();
        assert_eq!(keys[7], expected);
        assert_eq!(pairs[7].0, expected);
        let expected = Lookup::from_str("whomp[0]").unwrap();
        assert_eq!(keys[8], expected);
        assert_eq!(pairs[8].0, expected);
        let expected = Lookup::from_str("whomp[1]").unwrap();
        assert_eq!(keys[9], expected);
        assert_eq!(pairs[9].0, expected);
        let expected = Lookup::from_str("whomp[1].glomp").unwrap();
        assert_eq!(keys[10], expected);
        assert_eq!(pairs[10].0, expected);
        let expected = Lookup::from_str("whomp[1].glomp[0]").unwrap();
        assert_eq!(keys[11], expected);
        assert_eq!(pairs[11].0, expected);
        let expected = Lookup::from_str("whomp[1].glomp[1]").unwrap();
        assert_eq!(keys[12], expected);
        assert_eq!(pairs[12].0, expected);
        let expected = Lookup::from_str("zoop").unwrap();
        assert_eq!(keys[13], expected);
        assert_eq!(pairs[13].0, expected);

        Ok(())
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
