use crate::map::{IntoIter, Iter, IterMut, Map};
use std::collections::BTreeMap;
use std::iter::FromIterator;

impl<K, V> From<BTreeMap<K, V>> for Map<K, V>
where
    K: Ord + Clone,
    V: Clone,
{
    fn from(value: BTreeMap<K, V>) -> Self {
        let mut map = Map::new();
        for (k, v) in value {
            map.insert(k, v);
        }
        map
    }
}

impl<K, V> FromIterator<(K, V)> for Map<K, V>
where
    K: Ord,
{
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = (K, V)>,
    {
        let mut map = Map::default();
        for (k, v) in iter {
            map.insert(k, v);
        }
        map
    }
}

impl<K, V> IntoIterator for Map<K, V> {
    type Item = (K, V);
    type IntoIter = IntoIter<K, V>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            inner: self.inner.into_iter(),
        }
    }
}

impl<'a, K, V> IntoIterator for &'a Map<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        Iter {
            inner: self.inner.iter(),
        }
    }
}

impl<'a, K, V> IntoIterator for &'a mut Map<K, V>
where
    K: Clone,
    V: Clone,
{
    type Item = (&'a K, &'a mut V);
    type IntoIter = IterMut<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        IterMut {
            inner: self.inner.iter_mut(),
        }
    }
}
