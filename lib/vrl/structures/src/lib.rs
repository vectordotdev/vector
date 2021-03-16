use std::borrow::Borrow;
use std::cmp::Ordering;
use std::collections::btree_map;
use std::collections::BTreeMap;
use std::iter::FromIterator;

#[macro_export]
macro_rules! map {
    () => (::vrl_structures::Map::new());

    // trailing comma case
    ($($key:expr => $value:expr,)+) => (map!($($key => $value),+));

    ($($key:expr => $value:expr),*) => {
        {
            let mut _map = ::vrl_structures::Map::new();
            $(
                let _ = _map.insert($key.into(), $value.into());
            )*
            _map
        }
    };
}

#[derive(Debug, Clone)]
pub struct Map<K, V> {
    inner: Option<BTreeMap<K, V>>,
}

impl<K, V> PartialEq for Map<K, V>
where
    K: PartialEq<K>,
    V: PartialEq<V>,
{
    fn eq(&self, other: &Map<K, V>) -> bool {
        match (&self.inner, &other.inner) {
            (None, Some(omap)) => omap.is_empty(),
            (Some(smap), None) => smap.is_empty(),
            (None, None) => true,
            (Some(smap), Some(omap)) => {
                if smap.len() != other.len() {
                    return false;
                }

                self.iter().zip(omap).all(|(a, b)| a == b)
            }
        }
    }
}

pub struct Iter<'a, K, V> {
    inner: Option<btree_map::Iter<'a, K, V>>,
}

pub struct IterMut<'a, K, V> {
    inner: Option<btree_map::IterMut<'a, K, V>>,
}

pub struct IntoIter<K, V> {
    inner: Option<btree_map::IntoIter<K, V>>,
}

pub struct Keys<'a, K: 'a, V: 'a> {
    inner: Option<btree_map::Keys<'a, K, V>>,
}

pub struct Values<'a, K: 'a, V: 'a> {
    inner: Option<btree_map::Values<'a, K, V>>,
}

impl<K: Ord, V> From<BTreeMap<K, V>> for Map<K, V> {
    fn from(value: BTreeMap<K, V>) -> Self {
        let mut map = Map::new();
        for (k, v) in value {
            map.insert(k, v);
        }
        map
    }
}

impl<K: Eq, V: Eq> Eq for Map<K, V> {}

impl<K: PartialOrd, V: PartialOrd> PartialOrd for Map<K, V> {
    #[inline]
    fn partial_cmp(&self, other: &Map<K, V>) -> Option<Ordering> {
        self.iter().partial_cmp(other.iter())
    }
}

impl<K: Ord, V: Ord> Ord for Map<K, V> {
    #[inline]
    fn cmp(&self, other: &Map<K, V>) -> Ordering {
        self.iter().cmp(other.iter())
    }
}

impl<K, V> Map<K, V> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        if let Some(map) = &self.inner {
            map.len()
        } else {
            0
        }
    }

    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter {
            inner: self.inner.as_ref().map(|i| i.iter()),
        }
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, K, V> {
        IterMut {
            inner: self.inner.as_mut().map(|i| i.iter_mut()),
        }
    }

    pub fn keys(&self) -> Keys<K, V> {
        Keys {
            inner: self.inner.as_ref().map(|x| x.keys()),
        }
    }

    pub fn values(&self) -> Values<K, V> {
        Values {
            inner: self.inner.as_ref().map(|x| x.values()),
        }
    }

    pub fn is_empty(&self) -> bool {
        if let Some(map) = &self.inner {
            map.is_empty()
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.inner = None
    }
}

impl<K: Ord, V> Map<K, V> {
    pub fn append(&mut self, other: &mut Map<K, V>) {
        match (&mut self.inner, &mut other.inner) {
            (&mut None, &mut None) => {}
            (Some(_map), None) => {}
            (None, Some(omap)) => {
                let mut map = BTreeMap::default();
                map.append(omap);
                self.inner = Some(map);
            }
            (Some(map), Some(omap)) => map.append(omap),
        }
    }

    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        if let Some(map) = &self.inner {
            map.contains_key(key.borrow())
        } else {
            false
        }
    }

    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        if let Some(map) = &self.inner {
            map.get(key.borrow())
        } else {
            None
        }
    }

    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        if let Some(map) = &mut self.inner {
            map.get_mut(key.borrow())
        } else {
            None
        }
    }

    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        if let Some(map) = &mut self.inner {
            map.remove(key.borrow())
        } else {
            None
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V>
    where
        K: Ord,
    {
        if let Some(map) = &mut self.inner {
            map.insert(key, value)
        } else {
            let mut map = BTreeMap::default();
            map.insert(key, value);
            self.inner = Some(map);
            None
        }
    }
}

impl<K, V> Iterator for IntoIter<K, V> {
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(inner) = &mut self.inner {
            inner.next()
        } else {
            None
        }
    }
}

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(inner) = &mut self.inner {
            inner.next()
        } else {
            None
        }
    }
}

impl<'a, K, V> Iterator for IterMut<'a, K, V> {
    type Item = (&'a K, &'a mut V);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(inner) = &mut self.inner {
            inner.next()
        } else {
            None
        }
    }
}

impl<K, V> Default for Map<K, V> {
    fn default() -> Self {
        Self { inner: None }
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

impl<K, V> IntoIterator for Map<K, V> {
    type Item = (K, V);
    type IntoIter = IntoIter<K, V>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            inner: self.inner.map(|x| x.into_iter()),
        }
    }
}

impl<'a, K, V> IntoIterator for &'a Map<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        Iter {
            inner: self.inner.as_ref().map(|x| x.iter()),
        }
    }
}

impl<'a, K, V> IntoIterator for &'a mut Map<K, V> {
    type Item = (&'a K, &'a mut V);
    type IntoIter = IterMut<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        IterMut {
            inner: self.inner.as_mut().map(|x| x.iter_mut()),
        }
    }
}

impl<'a, K, V> Iterator for Keys<'a, K, V> {
    type Item = &'a K;

    fn next(&mut self) -> Option<&'a K> {
        if let Some(keys) = &mut self.inner {
            keys.next()
        } else {
            None
        }
    }
}

impl<'a, K, V> Iterator for Values<'a, K, V> {
    type Item = &'a V;

    fn next(&mut self) -> Option<&'a V> {
        if let Some(values) = &mut self.inner {
            values.next()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn fail() {
        assert!(false);
    }
}
