#[cfg(test)]
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::collections::{hash_map::DefaultHasher, BTreeMap};
use std::hash::{Hash, Hasher};

use indexmap::IndexSet;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vector_common::byte_size_of::ByteSizeOf;
use vector_config::{configurable_component, Configurable};

type TagValue = Option<String>;

/// Tag values for a metric series.  This may be empty, a single value, or a set of values. This is
/// used to provide the storage for `TagValueSet`.
#[derive(Clone, Configurable, Debug, Eq, PartialEq)]
pub enum TagValueSet {
    /// This represents a set containing a no value or a single value. This is stored separately to
    /// avoid the overhead of allocating a hash table for the common case of a single value for a
    /// tag.
    Single(#[configurable(transparent)] Option<TagValue>),
    /// This holds an actual set of values. This variant will be automatically created when a single
    /// value is added to, and reduced down to a single value when the length is reduced to 1.  An
    /// index set is used for this set, as it preserves the insertion order of the contained
    /// elements. This allows us to retrieve the last element inserted which in turn allows us to
    /// emulate the set having a single value.
    Set(#[configurable(transparent)] IndexSet<TagValue>),
}

impl Default for TagValueSet {
    fn default() -> Self {
        Self::Single(None)
    }
}

impl TagValueSet {
    /// Convert this set into a single value, mimicking the behavior of this set being just a plain
    /// single string while still storing all of the values.
    pub(crate) fn into_single(self) -> Option<String> {
        match self {
            Self::Single(tag) => tag.and_then(|tag| tag),
            Self::Set(set) => {
                let mut iter = set.into_iter();
                loop {
                    match iter.next_back() {
                        Some(Some(value)) => break Some(value),
                        Some(None) => continue,
                        None => break None,
                    }
                }
            }
        }
    }

    /// Get the "single" value of this set, mimicking the behavior of this set being just a plain
    /// single string while still storing all of the values.
    pub(crate) fn as_single(&self) -> Option<&str> {
        match self {
            Self::Single(tag) => tag.as_ref().and_then(Option::as_deref),
            Self::Set(set) => {
                let mut iter = set.iter();
                loop {
                    match iter.next_back() {
                        Some(Some(value)) => break Some(value),
                        Some(None) => continue,
                        None => break None,
                    }
                }
            }
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Self::Single(None) => true,
            Self::Single(Some(_)) | Self::Set(_) => false, // the `Set` variant will never be empty
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        match self {
            Self::Single(None) => 0,
            Self::Single(Some(_)) => 1,
            Self::Set(set) => set.len(),
        }
    }

    #[cfg(test)]
    fn contains<T>(&self, value: &T) -> bool
    where
        T: Eq + Hash + PartialEq<Option<String>>,
        TagValue: Borrow<T>,
    {
        match self {
            Self::Single(None) => false,
            Self::Single(Some(tag)) => value == tag,
            Self::Set(set) => set.contains(value),
        }
    }

    fn retain(&mut self, mut f: impl FnMut(Option<&str>) -> bool) {
        match self {
            Self::Single(None) => (),
            Self::Single(Some(tag)) => {
                if !f(tag.as_deref()) {
                    *self = Self::Single(None);
                }
            }
            Self::Set(set) => {
                set.retain(|value| f(value.as_deref()));
                match set.len() {
                    0 | 1 => *self = Self::Single(set.pop()),
                    _ => {}
                }
            }
        }
    }

    fn insert(&mut self, value: impl Into<TagValue>) -> bool {
        let value = value.into();
        match self {
            // Need to take ownership of the single value to optionally move it into a set.
            Self::Single(single) => match single.take() {
                None => {
                    *self = Self::Single(Some(value));
                    false
                }
                Some(tag) => {
                    if tag == value {
                        *self = Self::Single(Some(value));
                        true
                    } else {
                        *self = Self::Set(IndexSet::from([tag, value]));
                        false
                    }
                }
            },
            Self::Set(set) => {
                // If the value was previously present, we want to move it to become the last element. The
                // only way to do this is to remove any existing value.
                set.remove(&value);
                set.insert(value)
            }
        }
    }

    fn iter(&self) -> TagValueRefIter<'_> {
        match self {
            TagValueSet::Single(tag) => TagValueRefIter::Single(tag.as_ref()),
            TagValueSet::Set(set) => TagValueRefIter::Set(set.into_iter()),
        }
    }

    fn into_iter(self) -> TagValueIter {
        match self {
            Self::Single(tag) => TagValueIter::Single(tag),
            Self::Set(set) => TagValueIter::Set(set.into_iter()),
        }
    }
}

// The impl for `Hash` here follows the guarantees for the derived `PartialEq`, The resulting hash
// will always be the same if the contents compare equal, so we can ignore the clippy lint.
#[allow(clippy::derive_hash_xor_eq)]
impl Hash for TagValueSet {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        match self {
            Self::Single(tag) => tag.hash(hasher),
            Self::Set(set) => {
                // Hash each contained value individually and then combine the hash values using
                // exclusive-or. This results in the same value no matter what order the storage order.
                let combined = set
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
    }
}

impl Ord for TagValueSet {
    fn cmp(&self, that: &Self) -> Ordering {
        // TODO: This will give wrong answers much of the time when the set has more than one item,
        // but comparing hash sets for ordering is non-trivial and this at least gives _an_ answer.
        // This is required to provide an ordering on the metric series, which is only used by the
        // metrics sink buffer `sort_for_compression` and is hard to emulate there.
        self.as_single().cmp(&that.as_single())
    }
}

impl PartialOrd for TagValueSet {
    fn partial_cmp(&self, that: &Self) -> Option<Ordering> {
        Some(self.cmp(that))
    }
}

impl<const N: usize> From<[String; N]> for TagValueSet {
    fn from(values: [String; N]) -> Self {
        // See logic in `TagValueSet::insert` to why we can't just use `Self(values.into())`
        let mut result = Self::default();
        for value in values {
            result.insert(value);
        }
        result
    }
}

impl<const N: usize> From<[TagValue; N]> for TagValueSet {
    fn from(values: [TagValue; N]) -> Self {
        values.into_iter().collect()
    }
}

impl From<Vec<String>> for TagValueSet {
    fn from(values: Vec<String>) -> Self {
        let mut result = Self::default();
        for value in values {
            result.insert(value);
        }
        result
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

impl FromIterator<String> for TagValueSet {
    fn from_iter<T: IntoIterator<Item = String>>(values: T) -> Self {
        // See logic in `TagValueSet::insert` to why we can't just use `Self(values.into())`
        let mut result = Self::default();
        for value in values {
            result.insert(Some(value));
        }
        result
    }
}

pub enum TagValueIter {
    Single(Option<TagValue>),
    Set(<IndexSet<TagValue> as IntoIterator>::IntoIter),
}

impl Iterator for TagValueIter {
    type Item = Option<String>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Single(single) => single.take(),
            Self::Set(set) => set.next(),
        }
    }
}

pub enum TagValueRefIter<'a> {
    Single(Option<&'a TagValue>),
    Set(<&'a IndexSet<TagValue> as IntoIterator>::IntoIter),
}

impl<'a> Iterator for TagValueRefIter<'a> {
    type Item = Option<&'a str>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Single(single) => single.take().map(Option::as_deref),
            Self::Set(set) => set.next().map(Option::as_deref),
        }
    }
}

impl ByteSizeOf for TagValueSet {
    fn allocated_bytes(&self) -> usize {
        match self {
            Self::Single(tag) => tag.allocated_bytes(),
            Self::Set(set) => set.allocated_bytes(),
        }
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

    /// Iterate over references to all values of each tag.
    pub fn iter_all(&self) -> impl Iterator<Item = (&str, Option<&str>)> {
        self.0
            .iter()
            .flat_map(|(name, tags)| tags.iter().map(|tag| (name.as_ref(), tag)))
    }

    /// Iterate over references to a single value of each tag.
    pub fn iter_single(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0
            .iter()
            .filter_map(|(name, tags)| tags.as_single().map(|tag| (name.as_ref(), tag)))
    }

    /// Iterate over all values of each tag.
    pub fn into_iter_all(self) -> impl Iterator<Item = (String, Option<String>)> {
        self.0
            .into_iter()
            .flat_map(|(name, tags)| tags.into_iter().map(move |tag| (name.clone(), tag)))
    }

    /// Iterate over a single value of each tag.
    pub fn into_iter_single(self) -> impl Iterator<Item = (String, String)> {
        self.0
            .into_iter()
            .filter_map(|(name, tags)| tags.into_single().map(|tag| (name, tag)))
    }

    pub fn contains_key(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).and_then(TagValueSet::as_single)
    }

    pub fn insert(&mut self, name: String, value: String) -> Option<String> {
        self.0
            .insert(name, TagValueSet::from([value]))
            .and_then(TagValueSet::into_single)
    }

    pub fn set_multi_value_tag(&mut self, name: String, values: Vec<TagValue>) {
        self.0.insert(name, TagValueSet::from(values));
    }

    pub fn remove(&mut self, name: &str) -> Option<String> {
        self.0.remove(name).and_then(TagValueSet::into_single)
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    pub fn extend(&mut self, tags: impl IntoIterator<Item = (String, String)>) {
        for (key, value) in tags {
            self.0.entry(key).or_default().insert(value);
        }
    }

    pub fn retain(&mut self, mut f: impl FnMut(&str, &str) -> bool) {
        self.0.retain(|key, tags| {
            tags.retain(|tag| tag.map_or(false, |tag| f(key, tag)));
            !tags.is_empty()
        });
    }
}

impl From<BTreeMap<String, String>> for MetricTags {
    fn from(tags: BTreeMap<String, String>) -> Self {
        tags.into_iter().collect()
    }
}

impl From<BTreeMap<String, TagValue>> for MetricTags {
    fn from(tags: BTreeMap<String, TagValue>) -> Self {
        tags.into_iter().collect()
    }
}

impl<const N: usize> From<[(String, String); N]> for MetricTags {
    fn from(tags: [(String, String); N]) -> Self {
        tags.into_iter().collect()
    }
}

impl FromIterator<(String, String)> for MetricTags {
    fn from_iter<T: IntoIterator<Item = (String, String)>>(tags: T) -> Self {
        let mut result = Self::default();
        for (key, value) in tags {
            result.0.entry(key).or_default().insert(value);
        }
        result
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
            HashSet::<TagValue>::arbitrary(g).into_iter().collect()
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
        fn tag_value_nonbare_set_checks(values: Vec<String>, addition: String) {
            let mut set = TagValueSet::from(values.clone());
            assert_eq!(set.is_empty(), values.is_empty());
            // All input values are contained in the set.
            assert!(values.iter().all(|v| set.contains(&Some(v.clone()))));
            // All set values were in the input.
            assert!(set.iter().all(|s| s.map_or(true, |s| values.contains(&s.to_string()))));
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
            assert_eq!(new_addition, !set.contains(&Some(addition.clone())));
            set.insert(addition.clone());
            assert!(set.contains(&Some(addition.clone())));

            // If the addition wasn't in the start set, it will increase the length.
            assert_eq!(set.len(), start_len + if new_addition { 1 } else { 0 });
            // The "single" value will match the addition.
            assert_eq!(set.as_single(), Some(addition.as_str()));
        }
    }
}
