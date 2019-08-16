use super::{Value, ValueKind};
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Serialize, Serializer};
use std::collections::HashMap;
use string_cache::DefaultAtom as Atom;

lazy_static! {
    static ref ARRAY_RE: Regex = Regex::new(r"(?P<key>\D+)\[\d+\]").unwrap();
    static ref INDEX_RE: Regex = Regex::new(r"\[(?P<index>\d+)\]").unwrap();
}

#[derive(Debug, Clone, PartialEq)]
enum MapValue {
    Value(ValueKind),
    Map(HashMap<Atom, MapValue>),
    Array(Vec<MapValue>),
    Null,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Unflatten {
    map: HashMap<Atom, MapValue>,
}

impl From<HashMap<Atom, Value>> for Unflatten {
    fn from(log: HashMap<Atom, Value>) -> Self {
        let log = log
            .into_iter()
            .map(|(k, v)| (k, v.value))
            .collect::<HashMap<_, _>>();

        // We must wrap the outter map in a MapValue to support
        // the recursive merge.
        let mut map = MapValue::Map(HashMap::new());
        for (k, v) in log {
            let temp = unflatten(k, MapValue::Value(v));
            merge(&mut map, &temp);
        }

        if let MapValue::Map(map) = map {
            Unflatten { map }
        } else {
            unreachable!("unflatten always returns a map, this is a bug!");
        }
    }
}

/// This produces one path down the tree for each key that has
/// previously been flattened. The goal here is that the return value
/// of this function will be merged into the overall tree.
fn unflatten(k: Atom, v: MapValue) -> MapValue {
    // Maps are delimited via `.`.
    let mut s = k.rsplit(".").peekable();
    let mut map = HashMap::new();

    // Temp value variable that ends up representing the overall path of the tree.
    let mut temp_v = Some(v);
    // The first iteration of this will always be successful, since a split
    // on `.` will always produce at least one item even if the `.` is absent.
    //
    // We then continue to iterate through the split in reverse order to build
    // the `MapValue`'s.
    while let Some(mut k) = s.next() {
        // First, we must check to see if the key contans `[<index>]` indicating that
        // the inner item should actually be a `map<array<value>>`.
        if let Some(key) = ARRAY_RE.captures(&k).and_then(|c| c.name("key")) {
            let end = key.end();

            // We must also check if there are multiple arrays nested like so `key[0][1]`
            // and we must construct the inner arrays in reverse order.
            let indicies = INDEX_RE
                .captures_iter(&k[end - 1..])
                // This capture group index should always exist since we know its in
                // the regex. The parse should also pass since it is guarenteed to be a digit.
                .map(|c| c.name("index").unwrap().as_str().parse::<usize>().unwrap())
                .collect::<Vec<_>>();

            for i in indicies.into_iter().rev() {
                // Build an array where the temp_v will be placed at index `i`.
                let new_array = build_array(i, temp_v.take().unwrap());
                temp_v = Some(MapValue::Array(new_array));
            }

            // Return just the key that we parsed out.
            k = key.as_str()
        }

        // Check if there is another key ahead of this item or if our current `k`
        // is the root key. If it is the root key we just update the overall map
        // otherwise, we create an intermediate map that gets set as the the temp value.
        //
        // Since the next item is `None` in the iterator this essentially breaks out of the loop.
        if let None = s.peek() {
            map.insert(k.into(), temp_v.take().unwrap());
        } else {
            let mut m = HashMap::new();
            m.insert(k.into(), temp_v.take().unwrap());
            temp_v = Some(MapValue::Map(m));
        }
    }

    MapValue::Map(map)
}

/// Build an array placing the `value` at index `i`.
///
/// To allow placing the item at index `i`, we prefill the array up to
/// `i -1` with `MapValue::Null`, that will then get replaced.
fn build_array(i: usize, value: MapValue) -> Vec<MapValue> {
    let mut array = if i > 0 {
        (0..i)
            .into_iter()
            .map(|_| MapValue::Null)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    array.push(value);

    array
}

/// Merge `b` into `a` overwritting anything in `a` that conflicts.
// code borrowed from https://github.com/serde-rs/json/issues/377#issuecomment-341490464
fn merge(a: &mut MapValue, b: &MapValue) {
    match (a, b) {
        (&mut MapValue::Map(ref mut a), &MapValue::Map(ref b)) => {
            for (k, v) in b {
                merge(a.entry(k.clone()).or_insert(MapValue::Null), v);
            }
        }
        (&mut MapValue::Array(ref mut a), &MapValue::Array(ref b)) => {
            // Find all values and indexes that are _not_ `MapValue::Null`.
            for (i, v) in b.iter().enumerate().filter(|(_, e)| e != &&MapValue::Null) {
                // Determine if we need to reserve more space to avoid a panic on `Vec::insert`.
                // TODO: use `usize::checked_sub`
                if i > 0 && i >= a.len() {
                    let extra_capacity = i - a.len();
                    if extra_capacity > 0 {
                        a.reserve(extra_capacity);

                        // Any extra space needs to be filled with nulls.
                        for _ in 0..extra_capacity {
                            a.push(MapValue::Null)
                        }
                    }
                }

                let mut v = v.clone();

                // Attempt to merge the value with its current version,
                // before overwritting it.
                if let Some(a_v) = a.get(i) {
                    let mut a_v = a_v.clone();
                    merge(&mut a_v, &v);
                    v = a_v;
                }

                // Insert's do not remove the old item but shuffle them down.
                // to ensure that we do not add extra items to the vector we must
                // remove the previous item, since this item got merged above this is safe.
                if i < a.len() {
                    a.remove(i);
                }

                a.insert(i, v);
            }
        }
        (a, b) => {
            *a = b.clone();
        }
    }
}

impl Serialize for Unflatten {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_map(self.map.clone())
    }
}

impl Serialize for MapValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self {
            MapValue::Value(v) => v.serialize(serializer),
            MapValue::Map(m) => serializer.collect_map(m.clone()),
            MapValue::Array(a) => serializer.collect_seq(a.clone()),
            MapValue::Null => serializer.serialize_none(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::{self, Event},
        transforms::{
            json_parser::{JsonParser, JsonParserConfig},
            Transform,
        },
    };
    use serde::Deserialize;
    use std::collections::HashMap;

    #[test]
    fn merge_array() {
        let mut map1 = HashMap::new();
        let mut map2 = HashMap::new();

        map1.insert("key1".into(), MapValue::Value("v1".into()));
        map2.insert("key2".into(), MapValue::Value("v2".into()));

        let mut a = MapValue::Array(vec![MapValue::Map(map1.clone())]);
        let b = MapValue::Array(vec![MapValue::Map(map2.clone())]);

        merge(&mut a, &b);

        let mut map = HashMap::new();
        map.insert("key1".into(), MapValue::Value("v1".into()));
        map.insert("key2".into(), MapValue::Value("v2".into()));

        assert_eq!(a, MapValue::Array(vec![MapValue::Map(map)]));
    }

    #[test]
    fn nested_array() {
        let mut m = HashMap::new();
        let v = MapValue::Array(vec![MapValue::Array(vec![
            MapValue::Null,
            MapValue::Value("v1".into()),
        ])]);
        m.insert(Atom::from("a"), v);

        let output = unflatten("a[0][1]".into(), MapValue::Value("v1".into()));

        assert_eq!(output, MapValue::Map(m));
    }

    #[test]
    fn nested() {
        let mut e = Event::new_empty_log().into_log();
        e.insert_implicit("a.b.c".into(), "v1".into());
        e.insert_implicit("a.b.d".into(), "v2".into());

        let json = serde_json::to_string(&e.unflatten()).unwrap();
        let expected = serde_json::from_str::<Expected>(&json).unwrap();

        #[derive(Deserialize, Debug)]
        #[serde(rename_all = "snake_case")]
        struct Expected {
            a: A,
        }

        #[derive(Deserialize, Debug)]
        #[serde(rename_all = "snake_case")]
        struct A {
            b: B,
        }

        #[derive(Deserialize, Debug)]
        #[serde(rename_all = "snake_case")]
        struct B {
            c: String,
            d: String,
        }

        assert_eq!(&expected.a.b.c, "v1");
        assert_eq!(&expected.a.b.d, "v2");
    }

    #[test]
    fn array() {
        // We loop here as we want to ensure that we catch all corner cases
        // of hashmap iteration ordering.
        for _ in 0..100 {
            let mut e = Event::new_empty_log().into_log();
            e.insert_implicit("a.b[0]".into(), "v1".into());
            e.insert_implicit("a.b[1]".into(), "v2".into());

            #[derive(Deserialize, Debug)]
            #[serde(rename_all = "snake_case")]
            struct Expected {
                a: A,
            }

            #[derive(Deserialize, Debug)]
            #[serde(rename_all = "snake_case")]
            struct A {
                b: Vec<String>,
            }

            let json = serde_json::to_string(&e.unflatten()).unwrap();
            let expected = serde_json::from_str::<Expected>(&json).unwrap();

            assert_eq!(expected.a.b, vec!["v1", "v2"]);
        }
    }

    proptest::proptest! {
        #[test]
        fn unflatten_abirtrary(json in prop::json()) {
            let s = serde_json::to_string(&json).unwrap();
            let mut event = Event::new_empty_log();
            event.as_mut_log().insert_implicit(event::MESSAGE.clone().into(), s.into());

            let mut parser = JsonParser::from(JsonParserConfig::default());
            let event = parser.transform(event).unwrap().into_log();
            let expected_value = serde_json::to_value(&event.unflatten()).unwrap();

            assert_eq!(expected_value, json, "json: {}", serde_json::to_string_pretty(&json).unwrap());
        }
    }

    mod prop {
        use proptest::{
            arbitrary::any,
            collection::{hash_map, vec},
            prop_oneof,
            strategy::Strategy,
        };
        use serde_json::Value;

        /// This proptest strategy will randomly generate a
        /// `serde_json::Value` enum that represents different
        /// combinations of json objects.
        ///
        /// This will always produce at least an object at the root
        /// level. This is due to the fact that the root of unflatten is
        /// always a `MapValue::Map(..)`.
        ///
        /// The strategy will then recursively create random enum structures
        /// using the leaf strategy that only creates `bool`, `i64` and `[a-z]+`
        /// strings.
        ///
        /// It will then use a strategy of creating either a `vec` or a `hash_map`.
        pub fn json() -> impl Strategy<Value = Value> {
            let leaf = prop_oneof![
                any::<bool>().prop_map(Value::Bool),
                any::<i64>().prop_map(|n| Value::Number(n.into())),
                "[a-z]+".prop_map(Value::String),
            ];

            leaf.prop_recursive(8, 256, 10, |inner| {
                prop_oneof![
                    vec(inner.clone(), 1..10).prop_map(Value::Array),
                    hash_map("[a-z]+", inner, 1..10)
                        .prop_map(|m| Value::Object(m.into_iter().collect())),
                ]
            })
            .prop_map(|m| serde_json::json!({ "some": m }))
        }
    }
}
