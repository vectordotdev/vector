use std::collections::{btree_map, BTreeMap};

#[cfg(test)]
use quickcheck::{Arbitrary, Gen};
use vector_common::byte_size_of::ByteSizeOf;
use vector_config::configurable_component;

/// Tags for a metric series.
#[configurable_component]
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct MetricTags(#[configurable(transparent)] pub(in crate::event) BTreeMap<String, String>);

impl MetricTags {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_option(self) -> Option<Self> {
        (!self.is_empty()).then_some(self)
    }

    pub fn iter(&self) -> btree_map::Iter<'_, String, String> {
        self.0.iter()
    }

    pub fn contains_key(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }

    pub fn entry(&mut self, name: String) -> btree_map::Entry<String, String> {
        self.0.entry(name)
    }

    pub fn insert(&mut self, name: String, value: String) -> Option<String> {
        self.0.insert(name, value)
    }

    pub fn remove(&mut self, name: &str) -> Option<String> {
        self.0.remove(name)
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    pub fn extend(&mut self, tags: impl Iterator<Item = (String, String)>) {
        self.0.extend(tags);
    }

    pub fn retain(&mut self, mut f: impl FnMut(&str, &str) -> bool) {
        self.0.retain(|k, v| f(k, v));
    }
}

impl From<BTreeMap<String, String>> for MetricTags {
    fn from(tags: BTreeMap<String, String>) -> Self {
        Self(tags)
    }
}

impl IntoIterator for MetricTags {
    type Item = (String, String);

    type IntoIter = btree_map::IntoIter<String, String>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a MetricTags {
    type Item = (&'a str, &'a str);

    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        Iter(self.0.iter())
    }
}

pub struct Iter<'a>(btree_map::Iter<'a, String, String>);

impl<'a> Iterator for Iter<'a> {
    type Item = (&'a str, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

impl FromIterator<(String, String)> for MetricTags {
    fn from_iter<T: IntoIterator<Item = (String, String)>>(iter: T) -> Self {
        Self(BTreeMap::from_iter(iter))
    }
}

impl<const N: usize> From<[(String, String); N]> for MetricTags {
    fn from(tags: [(String, String); N]) -> Self {
        Self(BTreeMap::from(tags))
    }
}

impl ByteSizeOf for MetricTags {
    fn allocated_bytes(&self) -> usize {
        self.0.allocated_bytes()
    }
}

#[cfg(test)]
impl Arbitrary for MetricTags {
    fn arbitrary(g: &mut Gen) -> Self {
        Self(BTreeMap::arbitrary(g))
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(self.0.shrink().map(Self))
    }
}
