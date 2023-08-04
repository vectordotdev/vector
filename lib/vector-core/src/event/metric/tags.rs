#[cfg(test)]
use std::borrow::Borrow;

use std::borrow::Cow;
use std::collections::{hash_map::DefaultHasher, BTreeMap};
use std::fmt::Display;
use std::hash::{Hash, Hasher};
use std::{cmp::Ordering, mem};

use indexmap::IndexSet;
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize, Serializer};
use vector_common::byte_size_of::ByteSizeOf;
use vector_config::{configurable_component, Configurable};

/// A single tag value, either a bare tag or a value.
#[derive(Clone, Configurable, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(untagged)]
pub enum TagValue {
    /// Bare tag value.
    Bare,

    /// Tag value containing a string.
    Value(String),
}

impl From<String> for TagValue {
    fn from(value: String) -> Self {
        Self::Value(value)
    }
}

impl From<Option<String>> for TagValue {
    fn from(value: Option<String>) -> Self {
        match value {
            None => Self::Bare,
            Some(value) => Self::Value(value),
        }
    }
}

impl From<&str> for TagValue {
    fn from(value: &str) -> Self {
        Self::Value(value.to_string())
    }
}

impl From<Cow<'_, str>> for TagValue {
    fn from(value: Cow<'_, str>) -> Self {
        Self::Value(value.to_string())
    }
}

impl ByteSizeOf for TagValue {
    fn allocated_bytes(&self) -> usize {
        match self {
            Self::Bare => 0,
            Self::Value(value) => value.allocated_bytes(),
        }
    }
}

impl TagValue {
    pub fn as_option(&self) -> Option<&str> {
        match self {
            Self::Bare => None,
            Self::Value(value) => Some(value.as_str()),
        }
    }

    pub fn into_option(self) -> Option<String> {
        match self {
            Self::Bare => None,
            Self::Value(value) => Some(value),
        }
    }

    pub fn is_bare(&self) -> bool {
        matches!(self, Self::Bare)
    }

    pub fn is_value(&self) -> bool {
        matches!(self, Self::Value(_))
    }

    #[must_use]
    pub fn take(&mut self) -> Self {
        mem::replace(self, Self::Bare)
    }
}

type TagValueRef<'a> = Option<&'a str>;

/// Tag values for a metric series.  This may be empty, a single value, or a set of values. This is
/// used to provide the storage for `TagValueSet`.
#[derive(Clone, Configurable, Debug, Eq, PartialEq)]
pub enum TagValueSet {
    /// This represents a set containing no value.
    Empty,

    /// This represents a set containing a single value. This is stored separately to avoid the
    /// overhead of allocating a hash table for the common case of a single value for a tag.
    Single(TagValue),

    /// This holds an actual set of values. This variant will be automatically created when a single
    /// value is added to, and reduced down to a single value when the length is reduced to 1.  An
    /// index set is used for this set, as it preserves the insertion order of the contained
    /// elements. This allows us to retrieve the last element inserted which in turn allows us to
    /// emulate the set having a single value.
    Set(IndexSet<TagValue>),
}

impl Default for TagValueSet {
    fn default() -> Self {
        Self::Empty
    }
}

impl Display for TagValueSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, value) in self.iter().enumerate() {
            if i != 0 {
                write!(f, ", ")?;
            }
            if let Some(value) = value {
                write!(f, "\"{value}\"")?;
            } else {
                write!(f, "null")?;
            }
        }
        Ok(())
    }
}

impl TagValueSet {
    /// Convert this set into a single value, mimicking the behavior of this set being just a plain
    /// single string while still storing all of the values.
    pub fn into_single(self) -> Option<String> {
        match self {
            Self::Empty => None,
            Self::Single(tag) => tag.into_option(),
            Self::Set(set) => set
                .into_iter()
                .rfind(TagValue::is_value)
                .and_then(TagValue::into_option),
        }
    }

    /// Get the "single" value of this set, mimicking the behavior of this set being just a plain
    /// single string while still storing all of the values.
    pub fn as_single(&self) -> Option<&str> {
        match self {
            Self::Empty => None,
            Self::Single(tag) => tag.as_option(),
            Self::Set(set) => set
                .iter()
                .rfind(|tag| tag.is_value())
                .and_then(TagValue::as_option),
        }
    }

    /// Reduce this tag set to either a simple single tag or an empty set.
    fn reduce_to_simple(&mut self) {
        match self {
            Self::Empty => (),
            Self::Single(tag) => {
                if tag == &TagValue::Bare {
                    *self = Self::Empty;
                }
            }
            Self::Set(set) => {
                // Extract the last element of the set that has a value and convert it back into
                // self as a single value.
                *self = std::mem::take(set)
                    .into_iter()
                    .rfind(TagValue::is_value)
                    .map_or(Self::Empty, Self::Single);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::Empty => true,
            Self::Single(_) | Self::Set(_) => false, // the `Set` variant will never be empty
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::Single(_) => 1,
            Self::Set(set) => set.len(),
        }
    }

    #[cfg(test)]
    fn contains<T>(&self, value: &T) -> bool
    where
        T: Eq + Hash + PartialEq<TagValue>,
        TagValue: Borrow<T>,
    {
        match self {
            Self::Empty => false,
            Self::Single(tag) => value == tag,
            Self::Set(set) => set.contains(value),
        }
    }

    fn insert(&mut self, value: impl Into<TagValue>) -> bool {
        let value = value.into();
        match self {
            Self::Empty => {
                *self = Self::Single(value);
                false
            }
            Self::Single(single) => {
                // Need to take ownership of the single value to optionally move it into a set.
                let tag = single.take();
                if tag == value {
                    *self = Self::Single(value);
                    true
                } else {
                    *self = Self::Set(IndexSet::from([tag, value]));
                    false
                }
            }
            Self::Set(set) => {
                let (index, result) = set.insert_full(value);
                if !result {
                    // The value was already in the set, so move it to the last position.
                    set.move_index(index, set.len() - 1);
                }
                result
            }
        }
    }

    pub fn iter(&self) -> TagValueRefIter<'_> {
        self.into_iter()
    }
}

impl IntoIterator for TagValueSet {
    type IntoIter = TagValueIntoIter;
    type Item = TagValue;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            Self::Empty => TagValueIntoIter::Empty,
            Self::Single(tag) => TagValueIntoIter::Single(tag),
            Self::Set(set) => TagValueIntoIter::Set(set.into_iter()),
        }
    }
}

impl<'a> IntoIterator for &'a TagValueSet {
    type IntoIter = TagValueRefIter<'a>;
    type Item = TagValueRef<'a>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            TagValueSet::Empty => TagValueRefIter::Empty,
            TagValueSet::Single(tag) => TagValueRefIter::Single(tag),
            TagValueSet::Set(set) => TagValueRefIter::Set(set.into_iter()),
        }
    }
}

// The impl for `Hash` here follows the guarantees for the derived `PartialEq`, The resulting hash
// will always be the same if the contents compare equal, so we can ignore the clippy lint.
#[allow(clippy::derived_hash_with_manual_eq)]
impl Hash for TagValueSet {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        match self {
            Self::Empty => 0_u8.hash(hasher),
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
            result.insert(TagValue::Value(value));
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
            result.insert(TagValue::Value(value));
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

pub enum TagValueIntoIter {
    Empty,
    Single(TagValue),
    Set(<IndexSet<TagValue> as IntoIterator>::IntoIter),
}

impl Iterator for TagValueIntoIter {
    type Item = TagValue;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Empty => None,
            Self::Single(single) => {
                let value = single.take();
                *self = Self::Empty;
                Some(value)
            }
            Self::Set(set) => set.next(),
        }
    }
}

pub enum TagValueRefIter<'a> {
    Empty,
    Single(&'a TagValue),
    Set(<&'a IndexSet<TagValue> as IntoIterator>::IntoIter),
}

impl<'a> Iterator for TagValueRefIter<'a> {
    type Item = TagValueRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Empty => None,
            Self::Single(single) => {
                let result = single.as_option();
                *self = Self::Empty;
                Some(result)
            }
            Self::Set(set) => set.next().map(TagValue::as_option),
        }
    }
}

impl ByteSizeOf for TagValueSet {
    fn allocated_bytes(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::Single(tag) => tag.allocated_bytes(),
            Self::Set(set) => set.allocated_bytes(),
        }
    }
}

impl<'de> Deserialize<'de> for TagValueSet {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Variants {
            // Backwards compatibility for existing data
            String(String),
            // This is the new form of tag values
            Array(Vec<TagValue>),
        }

        Variants::deserialize(de).map(|v| match v {
            Variants::String(s) => Self::from([s]),
            Variants::Array(a) => Self::from(a),
        })
    }
}

impl Serialize for TagValueSet {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        match self.len() {
            // Serialize a single tag as before.
            1 => match self.as_single() {
                Some(s) => ser.serialize_str(s),
                None => ser.serialize_none(),
            },
            // Serialize all other sizes (including empty tag sets) as arrays.
            len => {
                let mut ser = ser.serialize_seq(Some(len))?;
                for value in self {
                    ser.serialize_element(&value)?;
                }
                ser.end()
            }
        }
    }
}

/// Tags for a metric series.
#[configurable_component]
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MetricTags(pub(in crate::event) BTreeMap<String, TagValueSet>);

impl MetricTags {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_option(self) -> Option<Self> {
        (!self.is_empty()).then_some(self)
    }

    /// Iterates over all the tag value sets
    pub fn iter_sets(&self) -> impl Iterator<Item = (&str, &TagValueSet)> {
        self.0.iter().map(|(key, value)| (key.as_str(), value))
    }

    /// Iterate over references to all values of each tag.
    pub fn iter_all(&self) -> impl Iterator<Item = (&str, TagValueRef<'_>)> {
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
    pub fn into_iter_all(self) -> impl Iterator<Item = (String, TagValue)> {
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

    /// Add a value to a tag. This does not replace any existing tags unless the value is a
    /// duplicate.
    pub fn insert(&mut self, name: String, value: impl Into<TagValue>) {
        self.0.entry(name).or_default().insert(value.into());
    }

    /// Replace all the values of a tag with a single value.
    pub fn replace(&mut self, name: String, value: impl Into<TagValue>) -> Option<String> {
        self.0
            .insert(name, TagValueSet::from([value.into()]))
            .and_then(TagValueSet::into_single)
    }

    pub fn set_multi_value(&mut self, name: String, values: impl IntoIterator<Item = TagValue>) {
        let x = TagValueSet::from_iter(values);
        self.0.insert(name, x);
    }

    pub fn remove(&mut self, name: &str) -> Option<String> {
        self.0.remove(name).and_then(TagValueSet::into_single)
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    pub fn extend(&mut self, tags: impl IntoIterator<Item = (String, String)>) {
        for (key, value) in tags {
            self.0
                .entry(key)
                .or_default()
                .insert(TagValue::Value(value));
        }
    }

    pub fn retain(&mut self, mut f: impl FnMut(&str, &mut TagValueSet) -> bool) {
        self.0.retain(|key, tags| f(key.as_str(), tags));
    }

    /// Reduces all the tag values to their single value, discarding any for which that value would
    /// be null.
    pub(super) fn reduce_to_single(&mut self) {
        self.0
            .iter_mut()
            .for_each(|(_, values)| values.reduce_to_simple());
        self.retain(|_, values| !values.is_empty());
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
            result
                .0
                .entry(key)
                .or_default()
                .insert(TagValue::Value(value));
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

    impl Arbitrary for TagValue {
        fn arbitrary(g: &mut Gen) -> Self {
            Self::from(Option::<String>::arbitrary(g))
        }
    }

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
        fn reduces_set_to_simple(mut values: TagValueSet) {
            values.reduce_to_simple();
            assert!(values.is_empty() || (values.len() == 1 && values.as_single().is_some()));
        }

        #[test]
        fn reduces_tags_to_single(mut tags: MetricTags) {
            tags.reduce_to_single();
            for (_, values) in tags.iter_sets() {
                assert!(values.is_empty() || (values.len() == 1 && values.as_single().is_some()));
            }
        }

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

        fn tag_value_nonbare_set_checks(values: Vec<String>, addition: String) {
            let mut set = TagValueSet::from(values.clone());
            assert_eq!(set.is_empty(), values.is_empty());
            // All input values are contained in the set.
            assert!(values.iter().all(|v| set.contains(&TagValue::Value(v.clone()))));
            // All set values were in the input.
            assert!(set.iter().all(
                |s| values.contains(&s.expect("must not contain bare tags").into())
            ));
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

                if let Some(first) = values.first() {
                    // Check that re-adding the last value doesn't change the set.
                    set.insert(TagValue::Value(values.last().unwrap().clone()));
                    assert_eq!(set.len(), start_len);
                    assert_eq!(set.as_single(), values.last().map(String::as_str));

                    // But re-adding the first value makes it the last.
                    set.insert(TagValue::Value(first.clone()));
                    assert_eq!(set.len(), start_len);
                    assert_eq!(set.as_single(), Some(first.as_str()));
                }
            }

            let new_addition = !values.iter().any(|v| v == &addition);
            let addition=TagValue::Value(addition);
            assert_eq!(new_addition, !set.contains(&addition));
            set.insert(addition.clone());
            assert!(set.contains(&addition));

            // If the addition wasn't in the start set, it will increase the length.
            assert_eq!(set.len(), start_len + usize::from(new_addition));
            // The "single" value will match the addition.
            assert_eq!(set.as_single(), addition.as_option());
        }

        #[test]
        fn tag_value_set_checks(values: Vec<TagValue>, addition: TagValue) {
            let mut set = TagValueSet::from(values.clone());
            assert_eq!(set.is_empty(), values.is_empty());
            // All input values are contained in the set.
            assert!(values.iter().all(|v| set.contains(v)));
            // All set values were in the input.
            assert!(set.iter().all(|s| values.contains(&TagValue::from(s.map(str::to_string)))));
            // Critical: the "single value" of the set is the last value added.
            let last_value = values.iter().rfind(|v| v.is_value());
            if let Some(last) = &last_value {
                assert_eq!(set.as_single(), last.as_option());
            }

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

                if let Some(first) = values.first() {
                    // Check that re-adding the last value doesn't change the set.
                    set.insert(values.last().unwrap().clone());
                    assert_eq!(set.len(), start_len);
                    if let Some(last) = &last_value {
                        assert_eq!(set.as_single(), last.as_option());
                    }

                    // But re-adding the first value makes it the last.
                    set.insert(first.clone());
                    assert_eq!(set.len(), start_len);
                    // And it shows up as the single value if it was not bare.
                    if let TagValue::Value(first) = first {
                        assert_eq!(set.as_single(), Some(first.as_str()));
                    }
                }
            }

            let new_addition = !values.iter().any(|v| v == &addition);
            assert_eq!(new_addition, !set.contains(&addition));
            set.insert(addition.clone());
            assert!(set.contains(&addition));

            // If the addition wasn't in the start set, it will increase the length.
            assert_eq!(set.len(), start_len + usize::from(new_addition));
            // The "single" value will match the addition.
            if addition.is_value() {
                assert_eq!(set.as_single(), addition.as_option());
            }
        }
    }
}
