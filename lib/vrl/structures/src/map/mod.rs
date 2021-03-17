use std::borrow::Borrow;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

mod convert;
mod iter;
mod macros;
#[cfg(test)]
mod tests;

pub use convert::*;
pub use iter::*;
pub use macros::*;

#[derive(Debug, Clone)]
pub struct Map<K, V> {
    pub(crate) inner: Arc<BTreeMap<K, V>>,
}

impl<K, V> Default for Map<K, V>
where
    K: Ord + Clone,
    V: Clone,
{
    fn default() -> Self {
        Self {
            inner: Arc::new(BTreeMap::new()),
        }
    }
}

impl<K, V> PartialEq for Map<K, V>
where
    K: Ord + PartialEq<K>,
    V: PartialEq<V>,
{
    fn eq(&self, other: &Map<K, V>) -> bool {
        self.inner.eq(&other.inner)
    }
}

impl<K, V> Eq for Map<K, V>
where
    K: Ord + Eq,
    V: Eq,
{
}

impl<K, V> PartialOrd for Map<K, V>
where
    K: Ord + PartialOrd + Clone,
    V: PartialOrd + Clone,
{
    #[inline]
    fn partial_cmp(&self, other: &Map<K, V>) -> Option<Ordering> {
        self.iter().partial_cmp(other.iter())
    }
}

impl<K, V> Ord for Map<K, V>
where
    K: Ord + Clone,
    V: Ord + Clone,
{
    #[inline]
    fn cmp(&self, other: &Map<K, V>) -> Ordering {
        self.iter().cmp(other.iter())
    }
}

impl<K, V> Map<K, V> where K: Ord {}

impl<K, V> Map<K, V>
where
    K: Ord + Clone,
    V: Clone,
{
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter {
            inner: self.inner.iter(),
        }
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, K, V> {
        IterMut {
            inner: Arc::make_mut(&mut self.inner).iter_mut(),
        }
    }

    pub fn keys(&self) -> Keys<K, V> {
        Keys {
            inner: self.inner.keys(),
        }
    }

    pub fn values(&self) -> Values<K, V> {
        Values {
            inner: self.inner.values(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn clear(&mut self) {
        Arc::make_mut(&mut self.inner).clear()
    }

    pub fn append(&mut self, other: &mut Map<K, V>) {
        let omap = Arc::make_mut(&mut other.inner);
        let smap = Arc::make_mut(&mut self.inner);

        smap.append(omap);
    }

    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.inner.contains_key(key.borrow())
    }

    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.inner.get(key.borrow())
    }

    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        Arc::make_mut(&mut self.inner).get_mut(key.borrow())
    }

    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        Arc::make_mut(&mut self.inner).remove(key.borrow())
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        Arc::make_mut(&mut self.inner).insert(key, value)
    }
}
