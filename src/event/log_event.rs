use crate::event::{lookup::Segment, util, Lookup, PathComponent, Value};
use remap::{Object, Path};
use serde::{Serialize, Serializer};
use std::{
    collections::{btree_map::Entry, BTreeMap, HashMap},
    convert::{TryFrom, TryInto},
    fmt::{Debug, Display},
    iter::FromIterator,
};

#[derive(PartialEq, Debug, Clone, Default)]
pub struct LogEvent {
    fields: BTreeMap<String, Value>,
}

impl LogEvent {
    #[instrument(level = "trace", skip(self, key), fields(key = %key.as_ref()))]
    pub fn get(&self, key: impl AsRef<str>) -> Option<&Value> {
        util::log::get(&self.fields, key.as_ref())
    }

    #[instrument(level = "trace", skip(self, key), fields(key = %key.as_ref()))]
    pub fn get_flat(&self, key: impl AsRef<str>) -> Option<&Value> {
        self.fields.get(key.as_ref())
    }

    #[instrument(level = "trace", skip(self, key), fields(key = %key.as_ref()))]
    pub fn get_mut(&mut self, key: impl AsRef<str>) -> Option<&mut Value> {
        util::log::get_mut(&mut self.fields, key.as_ref())
    }

    #[instrument(level = "trace", skip(self, key), fields(key = %key.as_ref()))]
    pub fn contains(&self, key: impl AsRef<str>) -> bool {
        util::log::contains(&self.fields, key.as_ref())
    }

    #[instrument(level = "trace", skip(self, key), fields(key = %key.as_ref()))]
    pub fn insert(
        &mut self,
        key: impl AsRef<str>,
        value: impl Into<Value> + Debug,
    ) -> Option<Value> {
        util::log::insert(&mut self.fields, key.as_ref(), value.into())
    }

    #[instrument(level = "trace", skip(self, key), fields(key = ?key))]
    pub fn insert_path<V>(&mut self, key: Vec<PathComponent>, value: V) -> Option<Value>
    where
        V: Into<Value> + Debug,
    {
        util::log::insert_path(&mut self.fields, key, value.into())
    }

    #[instrument(level = "trace", skip(self, key), fields(key = %key))]
    pub fn insert_flat<K, V>(&mut self, key: K, value: V)
    where
        K: Into<String> + Display,
        V: Into<Value> + Debug,
    {
        self.fields.insert(key.into(), value.into());
    }

    #[instrument(level = "trace", skip(self, key), fields(key = %key.as_ref()))]
    pub fn try_insert(&mut self, key: impl AsRef<str>, value: impl Into<Value> + Debug) {
        let key = key.as_ref();
        if !self.contains(key) {
            self.insert(key, value);
        }
    }

    #[instrument(level = "trace", skip(self, key), fields(key = %key.as_ref()))]
    pub fn remove(&mut self, key: impl AsRef<str>) -> Option<Value> {
        util::log::remove(&mut self.fields, key.as_ref(), false)
    }

    #[instrument(level = "trace", skip(self, key), fields(key = %key.as_ref()))]
    pub fn remove_prune(&mut self, key: impl AsRef<str>, prune: bool) -> Option<Value> {
        util::log::remove(&mut self.fields, key.as_ref(), prune)
    }

    #[instrument(level = "trace", skip(self))]
    pub fn keys<'a>(&'a self) -> impl Iterator<Item = String> + 'a {
        util::log::keys(&self.fields)
    }

    #[instrument(level = "trace", skip(self))]
    pub fn all_fields(&self) -> impl Iterator<Item = (String, &Value)> + Serialize {
        util::log::all_fields(&self.fields)
    }

    #[instrument(level = "trace", skip(self))]
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    #[instrument(level = "trace", skip(self))]
    pub fn as_map(&self) -> &BTreeMap<String, Value> {
        &self.fields
    }

    #[instrument(level = "trace", skip(self, lookup), fields(lookup = %lookup), err)]
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

impl<T> std::ops::Index<T> for LogEvent
where
    T: AsRef<str>,
{
    type Output = Value;

    fn index(&self, key: T) -> &Value {
        self.get(key.as_ref())
            .expect(&*format!("Key is not found: {:?}", key.as_ref()))
    }
}

impl<K, V> Extend<(K, V)> for LogEvent
where
    K: AsRef<str>,
    V: Into<Value>,
{
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        for (k, v) in iter {
            self.insert(k.as_ref(), v.into());
        }
    }
}

// Allow converting any kind of appropriate key/value iterator directly into a LogEvent.
impl<K: AsRef<str>, V: Into<Value>> FromIterator<(K, V)> for LogEvent {
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

impl Object for LogEvent {
    fn get(&self, path: &remap::Path) -> Result<Option<remap::Value>, String> {
        if path.is_root() {
            let iter = self
                .as_map()
                .clone()
                .into_iter()
                .map(|(k, v)| (k, v.into()));

            return Ok(Some(remap::Value::from_iter(iter)));
        }

        let value = path
            .to_alternative_strings()
            .iter()
            .find_map(|key| self.get(key))
            .cloned()
            .map(Into::into);

        Ok(value)
    }

    fn remove(&mut self, path: &Path, compact: bool) -> Result<Option<remap::Value>, String> {
        if path.is_root() {
            return Ok(Some(
                std::mem::take(&mut self.fields)
                    .into_iter()
                    .map(|(key, value)| (key, value.into()))
                    .collect::<BTreeMap<_, _>>()
                    .into(),
            ));
        }

        // loop until we find a path that exists.
        for key in path.to_alternative_strings() {
            if !self.contains(&key) {
                continue;
            }

            return Ok(self.remove_prune(&key, compact).map(Into::into));
        }

        Ok(None)
    }

    fn insert(&mut self, path: &Path, value: remap::Value) -> Result<(), String> {
        if path.is_root() {
            match value {
                remap::Value::Map(map) => {
                    *self = map
                        .into_iter()
                        .map(|(k, v)| (k, v.into()))
                        .collect::<BTreeMap<_, _>>()
                        .into();

                    return Ok(());
                }
                _ => return Err("tried to assign non-map value to event root path".to_owned()),
            }
        }

        if let Some(path) = path.to_alternative_strings().first() {
            self.insert(path, value);
        }

        Ok(())
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

    #[test]
    fn object_get() {
        use remap::{Field::*, Object, Path, Segment::*};
        use shared::btreemap;

        let cases = vec![
            (btreemap! {}, vec![], Ok(Some(btreemap! {}.into()))),
            (
                btreemap! { "foo" => "bar" },
                vec![],
                Ok(Some(btreemap! { "foo" => "bar" }.into())),
            ),
            (
                btreemap! { "foo" => "bar" },
                vec![Field(Regular("foo".to_owned()))],
                Ok(Some("bar".into())),
            ),
            (
                btreemap! { "foo" => "bar" },
                vec![Field(Regular("bar".to_owned()))],
                Ok(None),
            ),
            (
                btreemap! { "foo" => vec![btreemap! { "bar" => true }] },
                vec![
                    Field(Regular("foo".to_owned())),
                    Index(0),
                    Field(Regular("bar".to_owned())),
                ],
                Ok(Some(true.into())),
            ),
            (
                btreemap! { "foo" => btreemap! { "bar baz" => btreemap! { "baz" => 2 } } },
                vec![
                    Field(Regular("foo".to_owned())),
                    Coalesce(vec![
                        Regular("qux".to_owned()),
                        Quoted("bar baz".to_owned()),
                    ]),
                    Field(Regular("baz".to_owned())),
                ],
                Ok(Some(2.into())),
            ),
        ];

        for (value, segments, expect) in cases {
            let value: BTreeMap<String, Value> = value;
            let event = LogEvent::from(value);
            let path = Path::new_unchecked(segments);

            assert_eq!(Object::get(&event, &path), expect)
        }
    }

    #[test]
    fn object_insert() {
        use remap::{Field::*, Object, Path, Segment::*};
        use shared::btreemap;

        let cases = vec![
            (
                btreemap! { "foo" => "bar" },
                vec![],
                btreemap! { "baz" => "qux" }.into(),
                btreemap! { "baz" => "qux" },
                Ok(()),
            ),
            (
                btreemap! { "foo" => "bar" },
                vec![Field(Regular("foo".to_owned()))],
                "baz".into(),
                btreemap! { "foo" => "baz" },
                Ok(()),
            ),
            (
                btreemap! { "foo" => "bar" },
                vec![
                    Field(Regular("foo".to_owned())),
                    Index(2),
                    Field(Quoted("bar baz".to_owned())),
                    Field(Regular("a".to_owned())),
                    Field(Regular("b".to_owned())),
                ],
                true.into(),
                btreemap! {
                    "foo" => vec![
                        Value::Null,
                        Value::Null,
                        btreemap! {
                            "bar baz" => btreemap! { "a" => btreemap! { "b" => true } },
                        }.into()
                    ]
                },
                Ok(()),
            ),
            (
                btreemap! { "foo" => vec![0, 1, 2] },
                vec![Field(Regular("foo".to_owned())), Index(5)],
                "baz".into(),
                btreemap! {
                    "foo" => vec![
                        0.into(),
                        1.into(),
                        2.into(),
                        Value::Null,
                        Value::Null,
                        Value::from("baz"),
                    ],
                },
                Ok(()),
            ),
            (
                btreemap! { "foo" => "bar" },
                vec![Field(Regular("foo".to_owned())), Index(0)],
                "baz".into(),
                btreemap! { "foo" => vec!["baz"] },
                Ok(()),
            ),
            (
                btreemap! { "foo" => Value::Array(vec![]) },
                vec![Field(Regular("foo".to_owned())), Index(0)],
                "baz".into(),
                btreemap! { "foo" => vec!["baz"] },
                Ok(()),
            ),
            (
                btreemap! { "foo" => Value::Array(vec![0.into()]) },
                vec![Field(Regular("foo".to_owned())), Index(0)],
                "baz".into(),
                btreemap! { "foo" => vec!["baz"] },
                Ok(()),
            ),
            (
                btreemap! { "foo" => Value::Array(vec![0.into(), 1.into()]) },
                vec![Field(Regular("foo".to_owned())), Index(0)],
                "baz".into(),
                btreemap! { "foo" => Value::Array(vec!["baz".into(), 1.into()]) },
                Ok(()),
            ),
            (
                btreemap! { "foo" => Value::Array(vec![0.into(), 1.into()]) },
                vec![Field(Regular("foo".to_owned())), Index(1)],
                "baz".into(),
                btreemap! { "foo" => Value::Array(vec![0.into(), "baz".into()]) },
                Ok(()),
            ),
        ];

        for (object, segments, value, expect, result) in cases {
            let object: BTreeMap<String, Value> = object;
            let mut event = LogEvent::from(object);
            let expect = LogEvent::from(expect);
            let value: remap::Value = value;
            let path = Path::new_unchecked(segments);

            assert_eq!(Object::insert(&mut event, &path, value.clone()), result);
            assert_eq!(event, expect);
            assert_eq!(remap::Object::get(&event, &path), Ok(Some(value)));
        }
    }

    #[test]
    fn object_remove() {
        use remap::{Field::*, Object, Path, Segment::*};
        use shared::btreemap;

        let cases = vec![
            (
                btreemap! { "foo" => "bar" },
                vec![Field(Regular("foo".to_owned()))],
                false,
                Some(btreemap! {}.into()),
            ),
            (
                btreemap! { "foo" => "bar" },
                vec![Coalesce(vec![
                    Quoted("foo bar".to_owned()),
                    Regular("foo".to_owned()),
                ])],
                false,
                Some(btreemap! {}.into()),
            ),
            (
                btreemap! { "foo" => "bar", "baz" => "qux" },
                vec![],
                false,
                Some(btreemap! {}.into()),
            ),
            (
                btreemap! { "foo" => "bar", "baz" => "qux" },
                vec![],
                true,
                Some(btreemap! {}.into()),
            ),
            (
                btreemap! { "foo" => vec![0] },
                vec![Field(Regular("foo".to_owned())), Index(0)],
                false,
                Some(btreemap! { "foo" => Value::Array(vec![]) }.into()),
            ),
            (
                btreemap! { "foo" => vec![0] },
                vec![Field(Regular("foo".to_owned())), Index(0)],
                true,
                Some(btreemap! {}.into()),
            ),
            (
                btreemap! {
                    "foo" => btreemap! { "bar baz" => vec![0] },
                    "bar" => "baz",
                },
                vec![
                    Field(Regular("foo".to_owned())),
                    Field(Quoted("bar baz".to_owned())),
                    Index(0),
                ],
                false,
                Some(
                    btreemap! {
                        "foo" => btreemap! { "bar baz" => Value::Array(vec![]) },
                        "bar" => "baz",
                    }
                    .into(),
                ),
            ),
            (
                btreemap! {
                    "foo" => btreemap! { "bar baz" => vec![0] },
                    "bar" => "baz",
                },
                vec![
                    Field(Regular("foo".to_owned())),
                    Field(Quoted("bar baz".to_owned())),
                    Index(0),
                ],
                true,
                Some(btreemap! { "bar" => "baz" }.into()),
            ),
        ];

        for (object, segments, compact, expect) in cases {
            let mut event = LogEvent::from(object);
            let path = Path::new_unchecked(segments);
            let removed = Object::get(&event, &path).unwrap();

            assert_eq!(Object::remove(&mut event, &path, compact), Ok(removed));
            assert_eq!(Object::get(&event, &Path::root()), Ok(expect))
        }
    }
}
