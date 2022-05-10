//! Ordered associated object, mapping from string to `Value`.

use std::{
    cmp::Ordering,
    collections::{btree_map, BTreeMap},
    ops::{Deref, DerefMut},
};

#[cfg(feature = "serde")]
use serde::{Serialize, Serializer};

/// Wraps a string-keyed `BTreeMap`, similar to `ValueRgex` in purpose.
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct Object<V>(BTreeMap<String, V>);

impl<V> Object<V> {
    /// Create a new `Object`
    #[must_use]
    pub fn new() -> Self {
        Self(BTreeMap::default())
    }

    /// Insert key->value into the object
    #[allow(clippy::needless_pass_by_value)] // we want to match BTreeMap::insert signature
    pub fn insert<K>(&mut self, key: K, value: V) -> Option<V>
    where
        K: ToString,
    {
        self.0.insert(key.to_string(), value)
    }

    /// Return an iterator over kv pairs
    #[must_use]
    #[allow(clippy::needless_lifetimes)] // unclear how to elide the explicit
                                         // lifetimes here, clippy gives no hint
    pub fn iter<'a>(&'a self) -> Iter<'a, String, V> {
        Iter(self.0.iter())
    }
}

// #[cfg(feature = "lua")]
// impl<V> UserData for Object<V> {

// }

#[cfg(feature = "serde")]
impl<V> Serialize for Object<V>
where
    V: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_map(&self.0)
    }
}

/// Iteration over key/value pairs of the Object
#[derive(Clone)]
pub struct Iter<'a, K, V>(btree_map::Iter<'a, K, V>);

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<'a, K, V> From<btree_map::Iter<'a, K, V>> for Iter<'a, K, V> {
    fn from(iter: btree_map::Iter<'a, K, V>) -> Self {
        Self(iter)
    }
}

pub struct IntoIter<K, V>(btree_map::IntoIter<K, V>);

impl<V> IntoIterator for Object<V> {
    type Item = (String, V);
    type IntoIter = IntoIter<String, V>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter(self.0.into_iter())
    }
}

impl<K, V> Iterator for IntoIter<K, V> {
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<V> Default for Object<V> {
    fn default() -> Self {
        Self(BTreeMap::default())
    }
}

impl<V> Deref for Object<V> {
    type Target = BTreeMap<String, V>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<V> DerefMut for Object<V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<K, V, const N: usize> From<[(K, V); N]> for Object<V>
where
    K: ToString,
{
    fn from(arr: [(K, V); N]) -> Self {
        if N == 0 {
            return Self::default();
        }

        let mut map = BTreeMap::default();
        for (k, v) in arr {
            map.insert(k.to_string(), v);
        }
        Self(map)
    }
}

impl<V> FromIterator<(String, V)> for Object<V> {
    fn from_iter<I: IntoIterator<Item = (String, V)>>(iter: I) -> Self {
        let mut obj = Self::default();

        for (k, v) in iter {
            obj.0.insert(k, v);
        }

        obj
    }
}

impl<V> From<btree_map::IntoIter<String, V>> for Object<V> {
    fn from(iter: btree_map::IntoIter<String, V>) -> Self {
        Self(iter.collect::<BTreeMap<_, _>>())
    }
}

impl<V> PartialEq for Object<V>
where
    V: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<V> PartialOrd for Object<V>
where
    V: PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}
