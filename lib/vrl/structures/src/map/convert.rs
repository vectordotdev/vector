use crate::map::{IntoIter, Iter, IterMut, Map};
use std::collections::btree_map;
use std::collections::BTreeMap;
use std::iter::FromIterator;
use std::sync::Arc;

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
    K: Ord + Clone,
    V: Clone,
{
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = (K, V)>,
    {
        let mut map = BTreeMap::new();
        map.extend(iter);
        Self {
            inner: Arc::new(map),
        }
    }
}

impl<K, V> IntoIterator for Map<K, V>
where
    K: Clone,
    V: Clone,
{
    type Item = (K, V);
    type IntoIter = IntoIter<K, V>;

    fn into_iter(self) -> Self::IntoIter {
        let inner: btree_map::IntoIter<K, V> = (*self.inner).clone().into_iter();
        IntoIter { inner }
    }
}

impl<'a, K, V> IntoIterator for &'a Map<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        let inner: btree_map::Iter<'_, K, V> = (*self.inner).iter();
        Iter { inner }
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
        let inner: btree_map::IterMut<'_, K, V> = Arc::make_mut(&mut self.inner).iter_mut();
        IterMut { inner }
    }
}
