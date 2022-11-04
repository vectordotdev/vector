use std::cmp::Ordering;
use std::collections::{btree_map, hash_map::DefaultHasher, BTreeMap};
use std::hash::{Hash, Hasher};

use indexmap::IndexSet;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vector_common::byte_size_of::ByteSizeOf;
use vector_config::{configurable_component, Configurable};

type TagValue = String;

/// Tag values for a metric series.
#[derive(Clone, Configurable, Debug, Default, Eq, PartialEq)]
pub struct TagValueSet(#[configurable(transparent)] IndexSet<TagValue>);

impl TagValueSet {
    /// Convert this set into a single value, mimicking the behavior of this set being just a plain
    /// single string while still storing all of the values.
    pub fn into_single(self) -> Option<String> {
        self.0.into_iter().last()
    }

    /// Get the "single" value of this set, mimicking the behavior of this set being just a plain
    /// single string while still storing all of the values.
    pub fn as_single(&self) -> Option<&str> {
        self.0.iter().last().map(String::as_ref)
    }

    pub fn iter(&self) -> <&Self as IntoIterator>::IntoIter {
        self.into_iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn contains(&self, value: &str) -> bool {
        self.0.contains(value)
    }

    pub fn retain(&mut self, mut f: impl FnMut(&str) -> bool) {
        self.0.retain(|value| f(value.as_str()));
    }

    pub fn insert(&mut self, value: TagValue) -> bool {
        // If the value was previously present, we want to move it to become the last element. The
        // only way to do this is to remove any existing value.
        self.0.remove(&value);
        self.0.insert(value)
    }
}

// The impl for `Hash` here follows the guarantees for the derived `PartialEq`, The resulting hash
// will always be the same if the contents compare equal, so we can ignore the clippy lint.
#[allow(clippy::derive_hash_xor_eq)]
impl Hash for TagValueSet {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        // Hash each contained value individually and then combine the hash values using
        // exclusive-or. This results in the same value no matter what order the storage order.
        let combined = self
            .0
            .iter()
            .map(|tag| {
                let mut hasher = DefaultHasher::default();
                tag.hash(&mut hasher);
                hasher.finish()
            })
            .reduce(|a, b| a ^ b);
        combined.hash(hasher);
    }
}

impl Ord for TagValueSet {
    fn cmp(&self, that: &Self) -> Ordering {
        // TODO: This will give wrong answers much of the time when the set has more than one item,
        // but comparing hash sets for ordering is non-trivial and this at least gives _an_ answer.
        // This is required to provide an ordering on the metric series, which is used by the
        // metrics sink buffer `sort_for_compression` and is hard to emulate there.
        self.as_single().cmp(&that.as_single())
    }
}

impl PartialOrd for TagValueSet {
    fn partial_cmp(&self, that: &Self) -> Option<Ordering> {
        Some(self.cmp(that))
    }
}

impl<const N: usize> From<[TagValue; N]> for TagValueSet {
    fn from(values: [TagValue; N]) -> Self {
        values.into_iter().collect()
    }
}

impl From<Vec<TagValue>> for TagValueSet {
    fn from(values: Vec<TagValue>) -> Self {
        values.into_iter().collect()
    }
}

impl FromIterator<TagValue> for TagValueSet {
    fn from_iter<T: IntoIterator<Item = TagValue>>(values: T) -> Self {
        // See logic in `TagValueSet::insert` to why we can't just use `Self(values.into())`
        let mut result = Self::default();
        for value in values {
            result.insert(value);
        }
        result
    }
}

impl IntoIterator for TagValueSet {
    type IntoIter = <IndexSet<TagValue> as IntoIterator>::IntoIter;
    type Item = TagValue;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a TagValueSet {
    type IntoIter = <&'a IndexSet<TagValue> as IntoIterator>::IntoIter;
    type Item = &'a String;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl ByteSizeOf for TagValueSet {
    fn allocated_bytes(&self) -> usize {
        self.0.allocated_bytes()
    }
}

impl<'de> Deserialize<'de> for TagValueSet {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        // Deserialize from a single string only
        let s = String::deserialize(de)?;
        Ok(Self::from([s]))
    }
}

impl Serialize for TagValueSet {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        // Always serialize the tags as a single value
        match self.as_single() {
            Some(s) => ser.serialize_str(s),
            None => ser.serialize_none(),
        }
    }
}

/// Tags for a metric series.
#[configurable_component]
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MetricTags(
    #[configurable(transparent)] pub(in crate::event) BTreeMap<String, TagValueSet>,
);

impl MetricTags {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_option(self) -> Option<Self> {
        (!self.is_empty()).then_some(self)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0
            .iter()
            .flat_map(|(name, tags)| tags.iter().map(|tag| (name.as_ref(), tag.as_ref())))
    }

    pub fn contains_key(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).and_then(TagValueSet::as_single)
    }

    pub fn insert(&mut self, name: String, value: TagValue) -> Option<TagValue> {
        self.0
            .insert(name, TagValueSet::from([value]))
            .and_then(TagValueSet::into_single)
    }

    pub fn remove(&mut self, name: &str) -> Option<TagValue> {
        self.0.remove(name).and_then(TagValueSet::into_single)
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    pub fn extend(&mut self, tags: impl IntoIterator<Item = (String, TagValue)>) {
        for (key, value) in tags {
            self.0.entry(key).or_default().insert(value);
        }
    }

    pub fn retain(&mut self, mut f: impl FnMut(&str, &str) -> bool) {
        self.0.retain(|key, tags| {
            tags.retain(|tag| f(key, tag));
            !tags.is_empty()
        });
    }
}

impl IntoIterator for MetricTags {
    type Item = (String, TagValue);
    type IntoIter = IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            base: self.0.into_iter(),
            current: None,
        }
    }
}

pub struct IntoIter {
    base: btree_map::IntoIter<String, TagValueSet>,
    current: Option<(String, <TagValueSet as IntoIterator>::IntoIter)>,
}

impl Iterator for IntoIter {
    type Item = (String, TagValue);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match &mut self.current {
                Some((key, tag_set)) => {
                    if let Some(value) = tag_set.next() {
                        break Some((key.clone(), value));
                    }
                    self.current = None;
                }
                None => {
                    self.current = self
                        .base
                        .next()
                        .map(|(key, value)| (key, value.into_iter()));
                    if self.current.is_none() {
                        break None;
                    }
                }
            }
        }
    }
}

impl<'a> IntoIterator for &'a MetricTags {
    type Item = (&'a str, &'a str);
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        Iter {
            base: self.0.iter(),
            current: None,
        }
    }
}

pub struct Iter<'a> {
    base: btree_map::Iter<'a, String, TagValueSet>,
    current: Option<(&'a str, <&'a TagValueSet as IntoIterator>::IntoIter)>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = (&'a str, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match &mut self.current {
                Some((key, tag_set)) => {
                    if let Some(value) = tag_set.next() {
                        break Some((key, value));
                    }
                    self.current = None;
                }
                None => {
                    self.current = self
                        .base
                        .next()
                        .map(|(key, value)| (key.as_str(), value.iter()));
                    if self.current.is_none() {
                        break None;
                    }
                }
            }
        }
    }
}

impl From<BTreeMap<String, TagValue>> for MetricTags {
    fn from(tags: BTreeMap<String, TagValue>) -> Self {
        tags.into_iter().collect()
    }
}

impl<const N: usize> From<[(String, TagValue); N]> for MetricTags {
    fn from(tags: [(String, TagValue); N]) -> Self {
        tags.into_iter().collect()
    }
}

impl FromIterator<(String, TagValue)> for MetricTags {
    fn from_iter<T: IntoIterator<Item = (String, TagValue)>>(tags: T) -> Self {
        let mut result = Self::default();
        for (key, value) in tags {
            result.0.entry(key).or_default().insert(value);
        }
        result
    }
}

impl ByteSizeOf for MetricTags {
    fn allocated_bytes(&self) -> usize {
        self.0.allocated_bytes()
    }
}

#[cfg(test)]
mod test_support {
    use std::collections::HashSet;

    use quickcheck::{Arbitrary, Gen};

    use super::*;

    impl Arbitrary for TagValueSet {
        fn arbitrary(g: &mut Gen) -> Self {
            Self(HashSet::<TagValue>::arbitrary(g).into_iter().collect())
        }
    }

    impl Arbitrary for MetricTags {
        fn arbitrary(g: &mut Gen) -> Self {
            Self(BTreeMap::arbitrary(g))
        }

        fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
            Box::new(self.0.shrink().map(Self))
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #[test]
        fn eq_implies_hash_matches_proptest(values1: TagValueSet, values2: TagValueSet) {
            fn hash<T: Hash>(values: &T) -> u64 {
                let mut hasher = DefaultHasher::default();
                values.hash(&mut hasher);
                hasher.finish()
            }
            if values1 == values2{
                assert_eq!(hash(&values1), hash(&values2));
            }
        }

        #[test]
        fn tag_value_set_checks(values: Vec<TagValue>, addition: TagValue) {
            let mut set = TagValueSet::from(values.clone());
            assert_eq!(set.is_empty(), values.is_empty());
            // All input values are contained in the set.
            assert!(values.iter().all(|v| set.contains(v.as_str())));
            // All set values were in the input.
            assert!(set.iter().all(|v| values.contains(v)));
            // Critical: the "single value" of the set is the last value added.
            assert_eq!(set.as_single(), values.last().map(String::as_str));

            let start_len = set.len();

            // Is the input value set unique?
            if values
                .iter()
                .enumerate()
                .any(|(i, v1)| values[i + 1..].iter().any(|v2| v1 == v2))
            {
                // Input values are not unique, so the resulting set will be shorter.
                assert!(start_len < values.len());
            } else {
                // All input values were unique, so the resulting set will have all of them.
                assert_eq!(start_len, values.len());

                if !values.is_empty() {
                    // Check that re-adding the last value doesn't change the set.
                    set.insert(values.last().unwrap().clone());
                    assert_eq!(set.len(), start_len);
                    assert_eq!(set.as_single(), values.last().map(String::as_str));

                    // But re-adding the first value makes it the last.
                    set.insert(values.first().unwrap().clone());
                    assert_eq!(set.len(), start_len);
                    assert_eq!(set.as_single(), values.first().map(String::as_str));
                }
            }

            let new_addition = !values.iter().any(|v| v == &addition);
            assert_eq!(new_addition, !set.contains(&addition));
            set.insert(addition.clone());
            assert!(set.contains(&addition));

            // If the addition wasn't in the start set, it will increase the length.
            assert_eq!(set.len(), start_len + if new_addition { 1 } else { 0 });
            // The "single" value will match the addition.
            assert_eq!(set.as_single(), Some(addition.as_str()));
        }
    }
}
