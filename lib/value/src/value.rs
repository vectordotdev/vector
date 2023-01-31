//! Contains the main "Value" type for Vector and VRL, as well as helper methods.

mod convert;
mod crud;
mod display;
mod error;
mod iter;
mod path;
mod regex;
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

pub use crate::value::regex::ValueRegex;
use bytes::{Bytes, BytesMut};
use chrono::{DateTime, SecondsFormat, Utc};
pub use iter::{IterItem, ValueIter};
use lookup::lookup_v2::ValuePath;
use ordered_float::NotNan;

/// A boxed `std::error::Error`.
pub type StdError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// The main value type used in Vector events, and VRL.
#[derive(Debug, Clone)]
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

    /// Timestamp (UTC).
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
            (Self::Array(a), Self::Array(b)) => a.eq(b),
            (Self::Boolean(a), Self::Boolean(b)) => a.eq(b),
            (Self::Bytes(a), Self::Bytes(b)) => a.eq(b),
            (Self::Regex(a), Self::Regex(b)) => a.eq(b),
            (Self::Float(a), Self::Float(b)) => {
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
            (Self::Integer(a), Self::Integer(b)) => a.eq(b),
            (Self::Object(a), Self::Object(b)) => a.eq(b),
            (Self::Null, Self::Null) => true,
            (Self::Timestamp(a), Self::Timestamp(b)) => a.eq(b),
            _ => false,
        }
    }
}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            Self::Array(v) => {
                v.hash(state);
            }
            Self::Boolean(v) => {
                v.hash(state);
            }
            Self::Bytes(v) => {
                v.hash(state);
            }
            Self::Regex(regex) => {
                regex.as_bytes_slice().hash(state);
            }
            Self::Float(v) => {
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
            Self::Integer(v) => {
                v.hash(state);
            }
            Self::Object(v) => {
                v.hash(state);
            }
            Self::Null => {
                //covered by discriminant hash
            }
            Self::Timestamp(v) => {
                v.hash(state);
            }
        }
    }
}

impl Value {
    /// Returns a string description of the value type
    pub const fn kind_str(&self) -> &str {
        match self {
            Self::Bytes(_) | Self::Regex(_) => "string",
            Self::Timestamp(_) => "timestamp",
            Self::Integer(_) => "integer",
            Self::Float(_) => "float",
            Self::Boolean(_) => "boolean",
            Self::Object(_) => "map",
            Self::Array(_) => "array",
            Self::Null => "null",
        }
    }

    /// Merges `incoming` value into self.
    ///
    /// Will concatenate `Bytes` and overwrite the rest value kinds.
    pub fn merge(&mut self, incoming: Self) {
        match (self, incoming) {
            (Self::Bytes(self_bytes), Self::Bytes(ref incoming)) => {
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
            Self::Boolean(_)
            | Self::Bytes(_)
            | Self::Regex(_)
            | Self::Timestamp(_)
            | Self::Float(_)
            | Self::Integer(_) => false,
            Self::Null => true,
            Self::Object(v) => v.is_empty(),
            Self::Array(v) => v.is_empty(),
        }
    }

    /// Returns a reference to a field value specified by a path iter.
    #[allow(clippy::needless_pass_by_value)]
    pub fn insert<'a>(
        &mut self,
        path: impl ValuePath<'a>,
        insert_value: impl Into<Self>,
    ) -> Option<Self> {
        let insert_value = insert_value.into();
        let path_iter = path.segment_iter().peekable();

        crud::insert(self, (), path_iter, insert_value)
    }

    /// Removes field value specified by the given path and return its value.
    ///
    /// A special case worth mentioning: if there is a nested array and an item is removed
    /// from the middle of this array, then it is just replaced by `Value::Null`.
    #[allow(clippy::needless_pass_by_value)]
    pub fn remove<'a>(&mut self, path: impl ValuePath<'a>, prune: bool) -> Option<Self> {
        crud::remove(self, &(), path.segment_iter(), prune)
            .map(|(prev_value, _is_empty)| prev_value)
    }

    /// Returns a reference to a field value specified by a path iter.
    #[allow(clippy::needless_pass_by_value)]
    pub fn get<'a>(&self, path: impl ValuePath<'a>) -> Option<&Self> {
        crud::get(self, path.segment_iter())
    }

    /// Get a mutable borrow of the value by path
    #[allow(clippy::needless_pass_by_value)]
    pub fn get_mut<'a>(&mut self, path: impl ValuePath<'a>) -> Option<&mut Self> {
        crud::get_mut(self, path.segment_iter())
    }

    /// Determine if the lookup is contained within the value.
    pub fn contains<'a>(&self, path: impl ValuePath<'a>) -> bool {
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
    use lookup::lookup_v2::BorrowedSegment;
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
