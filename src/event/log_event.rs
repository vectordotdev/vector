use crate::event::{lookup::Segment, util, Lookup, PathComponent, Value};
use serde::{Serialize, Serializer};
use std::{
    collections::{btree_map::Entry, BTreeMap, HashMap},
    convert::{TryFrom, TryInto},
    iter::FromIterator,
};
use string_cache::DefaultAtom;

#[derive(PartialEq, Debug, Clone, Default)]
pub struct LogEvent {
    fields: BTreeMap<String, Value>,
}

impl LogEvent {
    pub fn get(&self, key: &DefaultAtom) -> Option<&Value> {
        util::log::get(&self.fields, key)
    }

    pub fn get_flat(&self, key: impl AsRef<str>) -> Option<&Value> {
        self.fields.get(key.as_ref())
    }

    pub fn get_mut(&mut self, key: &DefaultAtom) -> Option<&mut Value> {
        util::log::get_mut(&mut self.fields, key)
    }

    pub fn contains(&self, key: impl AsRef<str>) -> bool {
        util::log::contains(&self.fields, key.as_ref())
    }

    pub fn insert<K, V>(&mut self, key: K, value: V) -> Option<Value>
    where
        K: AsRef<str>,
        V: Into<Value>,
    {
        util::log::insert(&mut self.fields, key.as_ref(), value.into())
    }

    pub fn insert_path<V>(&mut self, key: Vec<PathComponent>, value: V) -> Option<Value>
    where
        V: Into<Value>,
    {
        util::log::insert_path(&mut self.fields, key, value.into())
    }

    pub fn insert_flat<K, V>(&mut self, key: K, value: V)
    where
        K: Into<String>,
        V: Into<Value>,
    {
        self.fields.insert(key.into(), value.into());
    }

    pub fn try_insert<V>(&mut self, key: &DefaultAtom, value: V)
    where
        V: Into<Value>,
    {
        if !self.contains(key) {
            self.insert(key.clone(), value);
        }
    }

    pub fn remove(&mut self, key: &DefaultAtom) -> Option<Value> {
        util::log::remove(&mut self.fields, &key, false)
    }

    pub fn remove_prune(&mut self, key: impl AsRef<str>, prune: bool) -> Option<Value> {
        util::log::remove(&mut self.fields, key.as_ref(), prune)
    }

    pub fn keys<'a>(&'a self) -> impl Iterator<Item = String> + 'a {
        util::log::keys(&self.fields)
    }

    pub fn all_fields(&self) -> impl Iterator<Item = (String, &Value)> + Serialize {
        util::log::all_fields(&self.fields)
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    #[instrument(skip(self, lookup), fields(lookup = %lookup), err)]
    fn entry(&mut self, lookup: Lookup) -> crate::Result<Entry<String, Value>> {
        trace!("Seeking to entry.");
        let mut walker = lookup.into_iter().enumerate();

        let mut current_pointer = if let Some((index, Segment::Field(segment))) = walker.next() {
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
                (Segment::Field(field), Entry::Occupied(entry)) => match entry.into_mut() {
                    Value::Map(map) => map.entry(field),
                    v => return Err(format!("Looking up field on a non-map value: {:?}", v).into()),
                },
                (Segment::Field(field), Entry::Vacant(entry)) => {
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

impl std::ops::Index<&DefaultAtom> for LogEvent {
    type Output = Value;

    fn index(&self, key: &DefaultAtom) -> &Value {
        self.get(key)
            .expect(&*format!("Key is not found: {:?}", key))
    }
}

impl<K: Into<DefaultAtom>, V: Into<Value>> Extend<(K, V)> for LogEvent {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        for (k, v) in iter {
            self.insert(k.into(), v.into());
        }
    }
}

// Allow converting any kind of appropriate key/value iterator directly into a LogEvent.
impl<K: Into<DefaultAtom>, V: Into<Value>> FromIterator<(K, V)> for LogEvent {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_util::open_fixture;
    use serde_json::json;
    use std::str::FromStr;
    use tracing::trace;

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

        let lookup = Lookup::from_str("non-existing").unwrap();
        let entry = event.entry(lookup).unwrap();
        let fallback = json!(
            "If you don't see this, the `LogEvent::entry` API is not working on non-existing lookups."
        );
        entry.or_insert_with(|| fallback.clone().into());
        let json: serde_json::Value = event.clone().try_into().unwrap();
        trace!(?json);
        assert_eq!(json.pointer("/non-existing"), Some(&fallback));

        let lookup = Lookup::from_str("nulled").unwrap();
        let entry = event.entry(lookup).unwrap();
        let fallback = json!(
            "If you see this, the `LogEvent::entry` API is not working on existing, single segment lookups."
        );
        entry.or_insert_with(|| fallback.clone().into());
        let json: serde_json::Value = event.clone().try_into().unwrap();
        assert_eq!(json.pointer("/nulled"), Some(&serde_json::Value::Null));

        let lookup = Lookup::from_str("map.basic").unwrap();
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

        let lookup = Lookup::from_str("map.map.buddy").unwrap();
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

        let lookup = Lookup::from_str("map.map.non-existing").unwrap();
        let entry = event.entry(lookup).unwrap();
        let fallback = json!(
            "If you don't see this, the `LogEvent::entry` API is not working on non-existing multi-segment lookups."
        );
        entry.or_insert_with(|| fallback.clone().into());
        let json: serde_json::Value = event.clone().try_into().unwrap();
        assert_eq!(json.pointer("/map/map/non-existing"), Some(&fallback));
    }
}
