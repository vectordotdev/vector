use crate::map::Pair;
use std::slice;
use std::vec;

pub struct Iter<'a, K, V> {
    pub(super) inner: slice::Iter<'a, Pair<K, V>>,
}

pub struct IterMut<'a, K, V> {
    pub(super) inner: slice::IterMut<'a, Pair<K, V>>,
}

pub struct IntoIter<K, V> {
    pub(super) inner: vec::IntoIter<Pair<K, V>>,
}

#[derive(Debug)]
pub struct Keys<'a, K: 'a, V: 'a> {
    pub(super) inner: slice::Iter<'a, Pair<K, V>>,
}

#[derive(Debug)]
pub struct Values<'a, K: 'a, V: 'a> {
    pub(super) inner: slice::Iter<'a, Pair<K, V>>,
}

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(pair) = self.inner.next() {
            if let Some(value) = &pair.value {
                return Some((&pair.key, value));
            }
        }
        None
    }
}

impl<'a, K, V> Iterator for IterMut<'a, K, V> {
    type Item = (&'a K, &'a mut V);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(pair) = self.inner.next() {
            if let Some(value) = &mut pair.value {
                return Some((&pair.key, value));
            }
        }
        None
    }
}

impl<K, V> Iterator for IntoIter<K, V> {
    type Item = (K, V);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(pair) = self.inner.next() {
            if pair.value.is_some() {
                return Some((pair.key, pair.value.unwrap()));
            }
        }
        None
    }
}

impl<'a, K, V> Iterator for Keys<'a, K, V> {
    type Item = &'a K;

    #[inline]
    fn next(&mut self) -> Option<&'a K> {
        while let Some(pair) = self.inner.next() {
            if pair.value.is_some() {
                return Some(&pair.key);
            }
        }
        None
    }
}

impl<'a, K, V> Iterator for Values<'a, K, V> {
    type Item = &'a V;

    #[inline]
    fn next(&mut self) -> Option<&'a V> {
        while let Some(pair) = self.inner.next() {
            if pair.value.is_some() {
                return pair.value.as_ref();
            }
        }
        None
    }
}
