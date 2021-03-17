use std::collections::btree_map;

pub struct Iter<'a, K, V> {
    pub(crate) inner: btree_map::Iter<'a, K, V>,
}

pub struct IterMut<'a, K, V> {
    pub(crate) inner: btree_map::IterMut<'a, K, V>,
}

pub struct IntoIter<K, V> {
    pub(crate) inner: btree_map::IntoIter<K, V>,
}

#[derive(Debug)]
pub struct Keys<'a, K: 'a, V: 'a> {
    pub(crate) inner: btree_map::Keys<'a, K, V>,
}

#[derive(Debug)]
pub struct Values<'a, K: 'a, V: 'a> {
    pub(crate) inner: btree_map::Values<'a, K, V>,
}

impl<K, V> Iterator for IntoIter<K, V> {
    type Item = (K, V);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<'a, K, V> Iterator for IterMut<'a, K, V> {
    type Item = (&'a K, &'a mut V);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<'a, K, V> Iterator for Keys<'a, K, V> {
    type Item = &'a K;

    #[inline]
    fn next(&mut self) -> Option<&'a K> {
        self.inner.next()
    }
}

impl<'a, K, V> Iterator for Values<'a, K, V> {
    type Item = &'a V;

    #[inline]
    fn next(&mut self) -> Option<&'a V> {
        self.inner.next()
    }
}
