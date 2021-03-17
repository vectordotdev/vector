use crate::map::Map;
use std::collections::BTreeMap;
use std::iter::FromIterator;

impl<K: Ord, V> From<BTreeMap<K, V>> for Map<K, V> {
    fn from(value: BTreeMap<K, V>) -> Self {
        let mut map = Map::new();
        for (k, v) in value {
            map.insert(k, v);
        }
        map
    }
}

impl<K: Ord, V> FromIterator<(K, V)> for Map<K, V> {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = (K, V)>,
    {
        let mut map = BTreeMap::new();
        map.extend(iter);
        Self { inner: Some(map) }
    }
}
