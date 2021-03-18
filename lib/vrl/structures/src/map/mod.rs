mod convert;
mod iter;
mod macros;
#[cfg(test)]
mod tests;

pub use convert::*;
pub use iter::*;
pub use macros::*;
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::mem;

#[derive(Debug, Clone)]
struct Pair<K, V> {
    key: K,
    value: Option<V>,
}

impl<K, V> PartialEq for Pair<K, V>
where
    K: PartialEq,
    V: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.value == other.value
    }
}

impl<K, V> Eq for Pair<K, V>
where
    K: PartialEq,
    V: PartialEq,
{
}

impl<K, V> PartialOrd for Pair<K, V>
where
    K: PartialOrd,
    V: PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match self.key.partial_cmp(&other.key) {
            Some(Ordering::Equal) => self.value.partial_cmp(&other.value),
            Some(ord) => Some(ord),
            None => None,
        }
    }
}

impl<K, V> Ord for Pair<K, V>
where
    K: Ord,
    V: Ord,
{
    fn cmp(&self, other: &Self) -> Ordering {
        self.key.cmp(&other.key).then(self.value.cmp(&other.value))
    }
}

#[derive(Debug, Clone)]
pub struct Map<K, V> {
    inner: Vec<Pair<K, V>>,
    length: usize,
}

impl<K, V> Default for Map<K, V>
where
    K: Ord,
{
    fn default() -> Self {
        Self {
            length: 0,
            inner: Vec::with_capacity(128),
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
    K: Ord + PartialOrd,
    V: PartialOrd,
{
    #[inline]
    fn partial_cmp(&self, other: &Map<K, V>) -> Option<Ordering> {
        self.iter().partial_cmp(other.iter())
    }
}

impl<K, V> Ord for Map<K, V>
where
    K: Ord,
    V: Ord,
{
    #[inline]
    fn cmp(&self, other: &Map<K, V>) -> Ordering {
        self.iter().cmp(other.iter())
    }
}

impl<K, V> Map<K, V> {
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.length
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    #[inline]
    pub fn clear(&mut self) {
        for pair in self.inner.iter_mut() {
            pair.value = None
        }
        self.length = 0
    }

    #[inline]
    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter {
            inner: self.inner.iter(),
        }
    }

    #[inline]
    pub fn iter_mut(&mut self) -> IterMut<'_, K, V> {
        IterMut {
            inner: self.inner.iter_mut(),
        }
    }

    #[inline]
    pub fn keys(&self) -> Keys<K, V> {
        Keys {
            inner: self.inner.iter(),
        }
    }

    #[inline]
    pub fn values(&self) -> Values<K, V> {
        Values {
            inner: self.inner.iter(),
        }
    }
}

impl<K, V> Map<K, V>
where
    K: Ord,
{
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn append(&mut self, other: &mut Map<K, V>) {
        for pair in other.inner.drain(..) {
            self.insert_pair(pair);
        }
    }

    #[inline]
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let kv = Pair {
            key,
            value: Some(value),
        };
        if let Some(pair) = self.insert_pair(kv) {
            pair.value
        } else {
            None
        }
    }

    #[inline]
    fn insert_pair(&mut self, pair: Pair<K, V>) -> Option<Pair<K, V>> {
        match self
            .inner
            .binary_search_by(|probe| probe.key.cmp(&pair.key))
        {
            Ok(idx) => {
                let old_pair = mem::replace(&mut self.inner[idx], pair);
                if old_pair.value.is_none() {
                    self.length += 1;
                }
                Some(old_pair)
            }
            Err(idx) => {
                self.inner.insert(idx, pair);
                self.length += 1;
                None
            }
        }
    }

    #[inline]
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.get(key).is_some()
    }

    #[inline]
    pub fn get<Q: ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        match self
            .inner
            .binary_search_by_key(&key, |pair| pair.key.borrow())
        {
            Ok(idx) => self.inner[idx].value.as_ref(),
            Err(_) => None,
        }
    }

    #[inline]
    pub fn get_mut<Q: ?Sized>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        match self
            .inner
            .binary_search_by_key(&key, |pair| pair.key.borrow())
        {
            Ok(idx) => self.inner[idx].value.as_mut(),
            Err(_) => None,
        }
    }

    #[inline]
    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        match self
            .inner
            .binary_search_by_key(&key, |pair| pair.key.borrow())
        {
            Ok(idx) => {
                let res = mem::replace(&mut self.inner[idx].value, None);
                if res.is_some() {
                    self.length -= 1;
                }
                res
            }
            Err(_) => None,
        }
    }
}
