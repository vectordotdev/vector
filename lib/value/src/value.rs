//! Contains the main "Value" type for Vector and VRL, as well as helper methods.

mod convert;
mod display;
mod error;
mod insert;
mod iter;
mod path;
mod regex;
mod remove;
mod target;

#[cfg(any(test, feature = "api"))]
mod api;
#[cfg(any(test, feature = "arbitrary"))]
mod arbitrary;
#[cfg(any(test, feature = "lua"))]
mod lua;
#[cfg(any(test, feature = "json"))]
mod serde;
#[cfg(any(test, feature = "toml"))]
mod toml;

use std::{
    collections::BTreeMap,
    fmt::Debug,
    hash::{Hash, Hasher},
};

use bytes::{Bytes, BytesMut};
use chrono::{DateTime, SecondsFormat, Utc};

pub use iter::IterItem;
use lookup::lookup_v2::{BorrowedSegment, Path};

use ordered_float::NotNan;

pub use crate::value::regex::ValueRegex;

/// A boxed `std::error::Error`.
pub type StdError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// The main value type used in Vector events, and VRL.
#[derive(PartialOrd, Debug, Clone)]
pub enum Value {
    /// Bytes - usually representing a UTF8 String.
    Bytes(Bytes),

    /// Regex.
    /// When used in the context of Vector this is treated identically to Bytes. It has
    /// additional meaning in the context of VRL.
    Regex(ValueRegex),

    /// Integer.
    Integer(i64),

    /// Float - not NaN.
    Float(NotNan<f64>),

    /// Boolean.
    Boolean(bool),

    /// Timetamp (UTC).
    Timestamp(DateTime<Utc>),

    /// Object.
    Object(BTreeMap<String, Value>),

    /// Array.
    Array(Vec<Value>),

    /// Null.
    Null,
}

impl Eq for Value {}

impl PartialEq<Self> for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Array(a), Value::Array(b)) => a.eq(b),
            (Value::Boolean(a), Value::Boolean(b)) => a.eq(b),
            (Value::Bytes(a), Value::Bytes(b)) => a.eq(b),
            (Value::Regex(a), Value::Regex(b)) => a.eq(b),
            (Value::Float(a), Value::Float(b)) => {
                // This compares floats with the following rules:
                // * NaNs compare as equal
                // * Positive and negative infinity are not equal
                // * -0 and +0 are not equal
                // * Floats will compare using truncated portion
                if a.is_sign_negative() == b.is_sign_negative() {
                    if a.is_finite() && b.is_finite() {
                        a.trunc().eq(&b.trunc())
                    } else {
                        a.is_finite() == b.is_finite()
                    }
                } else {
                    false
                }
            }
            (Value::Integer(a), Value::Integer(b)) => a.eq(b),
            (Value::Object(a), Value::Object(b)) => a.eq(b),
            (Value::Null, Value::Null) => true,
            (Value::Timestamp(a), Value::Timestamp(b)) => a.eq(b),
            _ => false,
        }
    }
}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            Value::Array(v) => {
                v.hash(state);
            }
            Value::Boolean(v) => {
                v.hash(state);
            }
            Value::Bytes(v) => {
                v.hash(state);
            }
            Value::Regex(regex) => {
                regex.as_bytes_slice().hash(state);
            }
            Value::Float(v) => {
                // This hashes floats with the following rules:
                // * NaNs hash as equal (covered by above discriminant hash)
                // * Positive and negative infinity has to different values
                // * -0 and +0 hash to different values
                // * otherwise transmute to u64 and hash
                if v.is_finite() {
                    v.is_sign_negative().hash(state);
                    let trunc: u64 = v.trunc().to_bits();
                    trunc.hash(state);
                } else if !v.is_nan() {
                    v.is_sign_negative().hash(state);
                } //else covered by discriminant hash
            }
            Value::Integer(v) => {
                v.hash(state);
            }
            Value::Object(v) => {
                v.hash(state);
            }
            Value::Null => {
                //covered by discriminant hash
            }
            Value::Timestamp(v) => {
                v.hash(state);
            }
        }
    }
}

impl Value {
    /// Returns a string description of the value type
    pub const fn kind_str(&self) -> &str {
        match self {
            Value::Bytes(_) | Value::Regex(_) => "string",
            Value::Timestamp(_) => "timestamp",
            Value::Integer(_) => "integer",
            Value::Float(_) => "float",
            Value::Boolean(_) => "boolean",
            Value::Object(_) => "map",
            Value::Array(_) => "array",
            Value::Null => "null",
        }
    }

    /// Merges `incoming` value into self.
    ///
    /// Will concatenate `Bytes` and overwrite the rest value kinds.
    pub fn merge(&mut self, incoming: Self) {
        match (self, incoming) {
            (Value::Bytes(self_bytes), Value::Bytes(ref incoming)) => {
                let mut bytes = BytesMut::with_capacity(self_bytes.len() + incoming.len());
                bytes.extend_from_slice(&self_bytes[..]);
                bytes.extend_from_slice(&incoming[..]);
                *self_bytes = bytes.freeze();
            }
            (current, incoming) => *current = incoming,
        }
    }

    /// Return if the node is empty, that is, it is an array or map with no items.
    ///
    /// ```rust
    /// use value::Value;
    /// use std::collections::BTreeMap;
    /// use lookup::path;
    ///
    /// let val = Value::from(1);
    /// assert_eq!(val.is_empty(), false);
    ///
    /// let mut val = Value::from(Vec::<Value>::default());
    /// assert_eq!(val.is_empty(), true);
    /// val.insert(path!(0), 1);
    /// assert_eq!(val.is_empty(), false);
    /// val.insert(path!(3), 1);
    /// assert_eq!(val.is_empty(), false);
    ///
    /// let mut val = Value::from(BTreeMap::default());
    /// assert_eq!(val.is_empty(), true);
    /// val.insert("foo", 1);
    /// assert_eq!(val.is_empty(), false);
    /// val.insert("bar", 2);
    /// assert_eq!(val.is_empty(), false);
    /// ```
    pub fn is_empty(&self) -> bool {
        match &self {
            Value::Boolean(_)
            | Value::Bytes(_)
            | Value::Regex(_)
            | Value::Timestamp(_)
            | Value::Float(_)
            | Value::Integer(_) => false,
            Value::Null => true,
            Value::Object(v) => v.is_empty(),
            Value::Array(v) => v.is_empty(),
        }
    }

    /// Returns a reference to a field value specified by a path iter.
    #[allow(clippy::needless_pass_by_value)]
    pub fn insert<'a>(
        &mut self,
        path: impl Path<'a>,
        insert_value: impl Into<Self>,
    ) -> Option<Self> {
        let insert_value = insert_value.into();
        let mut path_iter = path.segment_iter().peekable();

        match path_iter.peek() {
            None => Some(std::mem::replace(self, insert_value)),
            Some(BorrowedSegment::Field(field)) => {
                if let Self::Object(map) = self {
                    insert::map_insert(map, path_iter, insert_value)
                } else {
                    let mut map = BTreeMap::new();
                    let prev_value = insert::map_insert(&mut map, path_iter, insert_value);
                    *self = Self::Object(map);
                    prev_value
                }
            }
            Some(BorrowedSegment::Index(index)) => {
                if let Value::Array(array) = self {
                    insert::array_insert(array, path_iter, insert_value)
                } else {
                    let mut array = vec![];
                    let prev_value = insert::array_insert(&mut array, path_iter, insert_value);
                    *self = Self::Array(array);
                    prev_value
                }
            }
            Some(BorrowedSegment::Invalid) => None,
        }
    }

    /// Removes field value specified by the given path and return its value.
    ///
    /// A special case worth mentioning: if there is a nested array and an item is removed
    /// from the middle of this array, then it is just replaced by `Value::Null`.
    pub fn remove<'a>(&mut self, path: impl Path<'a>, prune: bool) -> Option<Self> {
        remove::remove(self, path, prune)
    }

    /// Returns a reference to a field value specified by a path iter.
    #[allow(clippy::needless_pass_by_value)]
    pub fn get<'a>(&self, path: impl Path<'a>) -> Option<&Self> {
        let mut value = self;
        let mut path_iter = path.segment_iter();
        loop {
            match (path_iter.next(), value) {
                (None, _) => return Some(value),
                (Some(BorrowedSegment::Field(key)), Value::Object(map)) => {
                    match map.get(key.as_ref()) {
                        None => return None,
                        Some(nested_value) => {
                            value = nested_value;
                        }
                    }
                }
                (Some(BorrowedSegment::Index(index)), Value::Array(array)) => {
                    match array.get(index as usize) {
                        None => return None,
                        Some(nested_value) => {
                            value = nested_value;
                        }
                    }
                }
                _ => return None,
            }
        }
    }

    /// Get a mutable borrow of the value by path
    #[allow(clippy::needless_pass_by_value)]
    pub fn get_mut<'a>(&mut self, path: impl Path<'a>) -> Option<&mut Self> {
        let mut value = self;
        let mut path_iter = path.segment_iter();
        loop {
            match (path_iter.next(), value) {
                (None, value) => return Some(value),
                (Some(BorrowedSegment::Field(key)), Value::Object(map)) => {
                    match map.get_mut(key.as_ref()) {
                        None => return None,
                        Some(nested_value) => {
                            value = nested_value;
                        }
                    }
                }
                (Some(BorrowedSegment::Index(index)), Value::Array(array)) => {
                    match array.get_mut(index as usize) {
                        None => return None,
                        Some(nested_value) => {
                            value = nested_value;
                        }
                    }
                }
                _ => return None,
            }
        }
    }

    /// Determine if the lookup is contained within the value.
    pub fn contains<'a>(&self, path: impl Path<'a>) -> bool {
        self.get(path).is_some()
    }
}

/// Converts a timestamp to a `String`.
#[must_use]
pub fn timestamp_to_string(timestamp: &DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

#[cfg(test)]
mod test {
    use lookup::path;
    use quickcheck::{QuickCheck, TestResult};

    use super::*;

    mod value_compare {
        use super::*;

        #[test]
        fn compare_correctly() {
            assert!(Value::Integer(0).eq(&Value::Integer(0)));
            assert!(!Value::Integer(0).eq(&Value::Integer(1)));
            assert!(!Value::Boolean(true).eq(&Value::Integer(2)));
            assert!(Value::from(1.2).eq(&Value::from(1.4)));
            assert!(!Value::from(1.2).eq(&Value::from(-1.2)));
            assert!(!Value::from(-0.0).eq(&Value::from(0.0)));
            assert!(!Value::from(f64::NEG_INFINITY).eq(&Value::from(f64::INFINITY)));
            assert!(Value::Array(vec![Value::Integer(0), Value::Boolean(true)])
                .eq(&Value::Array(vec![Value::Integer(0), Value::Boolean(true)])));
            assert!(!Value::Array(vec![Value::Integer(0), Value::Boolean(true)])
                .eq(&Value::Array(vec![Value::Integer(1), Value::Boolean(true)])));
        }
    }

    mod value_hash {
        use super::*;

        fn hash(a: &Value) -> u64 {
            let mut h = std::collections::hash_map::DefaultHasher::new();

            a.hash(&mut h);
            h.finish()
        }

        #[test]
        fn hash_correctly() {
            assert_eq!(hash(&Value::Integer(0)), hash(&Value::Integer(0)));
            assert_ne!(hash(&Value::Integer(0)), hash(&Value::Integer(1)));
            assert_ne!(hash(&Value::Boolean(true)), hash(&Value::Integer(2)));
            assert_eq!(hash(&Value::from(1.2)), hash(&Value::from(1.4)));
            assert_ne!(hash(&Value::from(1.2)), hash(&Value::from(-1.2)));
            assert_ne!(hash(&Value::from(-0.0)), hash(&Value::from(0.0)));
            assert_ne!(
                hash(&Value::from(f64::NEG_INFINITY)),
                hash(&Value::from(f64::INFINITY))
            );
            assert_eq!(
                hash(&Value::Array(vec![Value::Integer(0), Value::Boolean(true)])),
                hash(&Value::Array(vec![Value::Integer(0), Value::Boolean(true)]))
            );
            assert_ne!(
                hash(&Value::Array(vec![Value::Integer(0), Value::Boolean(true)])),
                hash(&Value::Array(vec![Value::Integer(1), Value::Boolean(true)]))
            );
        }
    }

    mod insert_get_remove {
        use super::*;

        #[test]
        fn single_field() {
            let mut value = Value::from(BTreeMap::default());
            let key = "root";
            let mut marker = Value::from(true);
            assert_eq!(value.insert(key, marker.clone()), None);
            assert_eq!(value.as_object().unwrap()[key], marker);
            assert_eq!(value.get(key), Some(&marker));
            assert_eq!(value.get_mut(key), Some(&mut marker));
            assert_eq!(value.remove(key, false), Some(marker));
        }

        #[test]
        fn nested_field() {
            let mut value = Value::from(BTreeMap::default());
            let key = "root.doot";
            let mut marker = Value::from(true);
            assert_eq!(value.insert(key, marker.clone()), None);
            assert_eq!(
                value.as_object().unwrap()["root"].as_object().unwrap()["doot"],
                marker
            );
            assert_eq!(value.get(key), Some(&marker));
            assert_eq!(value.get_mut(key), Some(&mut marker));
            assert_eq!(value.remove(key, false), Some(marker));
        }

        #[test]
        fn double_nested_field() {
            let mut value = Value::from(BTreeMap::default());
            let key = "root.doot.toot";
            let mut marker = Value::from(true);
            assert_eq!(value.insert(key, marker.clone()), None);
            assert_eq!(
                value.as_object().unwrap()["root"].as_object().unwrap()["doot"]
                    .as_object()
                    .unwrap()["toot"],
                marker
            );
            assert_eq!(value.get(key), Some(&marker));
            assert_eq!(value.get_mut(key), Some(&mut marker));
            assert_eq!(value.remove(key, false), Some(marker));
        }

        #[test]
        fn single_index() {
            let mut value = Value::from(Vec::<Value>::default());
            let key = "[0]";
            let mut marker = Value::from(true);
            assert_eq!(value.insert(key, marker.clone()), None);
            assert_eq!(value.as_array_unwrap()[0], marker);
            assert_eq!(value.get(key), Some(&marker));
            assert_eq!(value.get_mut(key), Some(&mut marker));
            assert_eq!(value.remove(key, false), Some(marker));
        }

        #[test]
        fn nested_index() {
            let mut value = Value::from(Vec::<Value>::default());
            let key = "[0][0]";
            let mut marker = Value::from(true);
            assert_eq!(value.insert(key, marker.clone()), None);
            assert_eq!(value.as_array_unwrap()[0].as_array_unwrap()[0], marker);
            assert_eq!(value.get(key), Some(&marker));
            assert_eq!(value.get_mut(key), Some(&mut marker));
            assert_eq!(value.remove(key, false), Some(marker));
        }

        #[test]
        fn field_index() {
            let mut value = Value::from(BTreeMap::default());
            let key = "root[0]";
            let mut marker = Value::from(true);
            assert_eq!(value.insert(key, marker.clone()), None);
            assert_eq!(
                value.as_object().unwrap()["root"].as_array_unwrap()[0],
                marker
            );
            assert_eq!(value.get(key), Some(&marker));
            assert_eq!(value.get_mut(key), Some(&mut marker));
            assert_eq!(value.remove(key, false), Some(marker));
        }

        #[test]
        fn index_field() {
            let mut value = Value::from(Vec::<Value>::default());
            let key = "[0].boot";
            let mut marker = Value::from(true);
            assert_eq!(value.insert(key, marker.clone()), None);
            assert_eq!(
                value.as_array_unwrap()[0].as_object().unwrap()["boot"],
                marker
            );
            assert_eq!(value.get(key), Some(&marker));
            assert_eq!(value.get_mut(key), Some(&mut marker));
            assert_eq!(value.remove(key, false), Some(marker));
        }

        #[test]
        fn nested_index_field() {
            let mut value = Value::from(Vec::<Value>::default());
            let key = "[0][0].boot";
            let mut marker = Value::from(true);
            assert_eq!(value.insert(key, marker.clone()), None);
            assert_eq!(
                value.as_array_unwrap()[0].as_array_unwrap()[0]
                    .as_object()
                    .unwrap()["boot"],
                marker
            );
            assert_eq!(value.get(key), Some(&marker));
            assert_eq!(value.get_mut(key), Some(&mut marker));
            assert_eq!(value.remove(key, false), Some(marker));
        }
        #[test]
        fn field_with_nested_index_field() {
            let mut value = Value::from(BTreeMap::default());
            let key = "root[0][0].boot";
            let mut marker = Value::from(true);
            assert_eq!(value.insert(key, marker.clone()), None);
            assert_eq!(
                value.as_object().unwrap()["root"].as_array_unwrap()[0].as_array_unwrap()[0]
                    .as_object()
                    .unwrap()["boot"],
                marker
            );
            assert_eq!(value.get(key), Some(&marker));
            assert_eq!(value.get_mut(key), Some(&mut marker));
            assert_eq!(value.remove(key, false), Some(marker));
        }

        #[test]
        fn populated_field() {
            let mut value = Value::from(BTreeMap::default());
            let marker = Value::from(true);
            assert_eq!(value.insert("a[2]", marker.clone()), None);

            let key = "a[0]";
            assert_eq!(value.insert(key, marker.clone()), Some(Value::Null));

            assert_eq!(value.as_object().unwrap()["a"].as_array_unwrap().len(), 3);
            assert_eq!(value.as_object().unwrap()["a"].as_array_unwrap()[0], marker);
            assert_eq!(
                value.as_object().unwrap()["a"].as_array_unwrap()[1],
                Value::Null
            );
            assert_eq!(value.as_object().unwrap()["a"].as_array_unwrap()[2], marker);

            // Replace the value at 0.
            let marker = Value::from(false);
            assert_eq!(value.insert(key, marker.clone()), Some(Value::from(true)));
            assert_eq!(value.as_object().unwrap()["a"].as_array_unwrap()[0], marker);
        }
    }

    mod corner_cases {
        use super::*;

        #[test]
        fn remove_prune_map_with_map() {
            let mut value = Value::from(BTreeMap::default());
            let key = "foo.bar";
            let marker = Value::from(true);
            assert_eq!(value.insert(key, marker.clone()), None);
            // Since the `foo` map is now empty, this should get cleaned.
            assert_eq!(value.remove(key, true), Some(marker));
            assert!(!value.contains("foo"));
        }

        #[test]
        fn remove_prune_map_with_array() {
            let mut value = Value::from(BTreeMap::default());
            let key = "foo[0]";
            let marker = Value::from(true);
            assert_eq!(value.insert(key, marker.clone()), None);
            // Since the `foo` map is now empty, this should get cleaned.
            assert_eq!(value.remove(key, true), Some(marker));
            assert!(!value.contains("foo"));
        }

        #[test]
        fn remove_prune_array_with_map() {
            let mut value = Value::from(Vec::<Value>::default());
            let key = "[0].bar";
            let marker = Value::from(true);
            assert_eq!(value.insert(key, marker.clone()), None);
            // Since the `foo` map is now empty, this should get cleaned.
            assert_eq!(value.remove(key, true), Some(marker));
            assert!(!value.contains(path!(0)));
        }

        #[test]
        fn remove_prune_array_with_array() {
            let mut value = Value::from(Vec::<Value>::default());
            let key = "[0][0]";
            let marker = Value::from(true);
            assert_eq!(value.insert(key, marker.clone()), None);
            // Since the `foo` map is now empty, this should get cleaned.
            assert_eq!(value.remove(key, true), Some(marker));
            assert!(!value.contains(path!(0)));
        }
    }

    #[test]
    fn quickcheck_value() {
        fn inner(mut path: Vec<BorrowedSegment<'static>>) -> TestResult {
            let mut value = Value::from(BTreeMap::default());
            let mut marker = Value::from(true);

            // Push a field at the start of the path so the top level is a map.
            path.insert(0, BorrowedSegment::from("field"));

            assert_eq!(value.insert(&path, marker.clone()), None, "inserting value");
            assert_eq!(value.get(&path), Some(&marker), "retrieving value");
            assert_eq!(
                value.get_mut(&path),
                Some(&mut marker),
                "retrieving mutable value"
            );

            assert_eq!(value.remove(&path, true), Some(marker), "removing value");

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .max_tests(200)
            .quickcheck(inner as fn(Vec<BorrowedSegment<'static>>) -> TestResult);
    }
}
