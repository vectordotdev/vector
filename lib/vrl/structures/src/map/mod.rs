use std::borrow::Borrow;
use std::cmp::Ordering;
use std::collections::BTreeMap;

mod iter;
mod macros;
#[cfg(test)]
mod tests;

pub use iter::*;
pub use macros::*;

#[derive(Debug, Clone)]
pub struct Map<K, V> {
    pub(crate) inner: Option<BTreeMap<K, V>>,
}

impl<K, V> Default for Map<K, V> {
    fn default() -> Self {
        Self { inner: None }
    }
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
            (Some(smap), Some(omap)) => smap.eq(omap),
        }
    }
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
            true
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
