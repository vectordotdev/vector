use crate::map::Map;
use std::collections::btree_map;

pub struct Iter<'a, K, V> {
    pub(crate) inner: Option<btree_map::Iter<'a, K, V>>,
}

pub struct IterMut<'a, K, V> {
    pub(crate) inner: Option<btree_map::IterMut<'a, K, V>>,
}

pub struct IntoIter<K, V> {
    pub(crate) inner: Option<btree_map::IntoIter<K, V>>,
}

#[derive(Debug)]
pub struct Keys<'a, K: 'a, V: 'a> {
    pub(crate) inner: Option<btree_map::Keys<'a, K, V>>,
}

#[derive(Debug)]
pub struct Values<'a, K: 'a, V: 'a> {
    pub(crate) inner: Option<btree_map::Values<'a, K, V>>,
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
