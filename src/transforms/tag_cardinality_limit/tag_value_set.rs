use crate::event::metric::TagValueSet;
use crate::transforms::tag_cardinality_limit::config::Mode;
use bloomy::BloomFilter;
use std::collections::HashSet;
use std::fmt;

/// Container for storing the set of accepted values for a given tag key.
#[derive(Debug)]
pub struct AcceptedTagValueSet {
    storage: TagValueSetStorage,
}

enum TagValueSetStorage {
    Set(HashSet<TagValueSet>),
    Bloom(BloomFilterStorage),
}

/// A bloom filter that tracks the number of items inserted into it.
struct BloomFilterStorage {
    inner: BloomFilter<TagValueSet>,

    /// Count of items inserted into the bloom filter.
    /// We manually track this because `BloomFilter::count` has O(n) time complexity.
    count: usize,
}

impl BloomFilterStorage {
    fn new(size: usize) -> Self {
        Self {
            inner: BloomFilter::with_size(size),
            count: 0,
        }
    }

    fn insert(&mut self, value: &TagValueSet) {
        // Only update the count if the value is not already in the bloom filter.
        if !self.inner.contains(value) {
            self.inner.insert(value);
            self.count += 1;
        }
    }

    fn contains(&self, value: &TagValueSet) -> bool {
        self.inner.contains(value)
    }

    const fn count(&self) -> usize {
        self.count
    }
}

impl fmt::Debug for TagValueSetStorage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TagValueSetStorage::Set(set) => write!(f, "Set({:?})", set),
            TagValueSetStorage::Bloom(_) => write!(f, "Bloom"),
        }
    }
}

impl AcceptedTagValueSet {
    pub fn new(value_limit: usize, mode: &Mode) -> Self {
        let storage = match &mode {
            Mode::Exact => TagValueSetStorage::Set(HashSet::with_capacity(value_limit)),
            Mode::Probabilistic(config) => {
                TagValueSetStorage::Bloom(BloomFilterStorage::new(config.cache_size_per_key))
            }
        };
        Self { storage }
    }

    pub fn contains(&self, value: &TagValueSet) -> bool {
        match &self.storage {
            TagValueSetStorage::Set(set) => set.contains(value),
            TagValueSetStorage::Bloom(bloom) => bloom.contains(value),
        }
    }

    pub fn len(&self) -> usize {
        match &self.storage {
            TagValueSetStorage::Set(set) => set.len(),
            TagValueSetStorage::Bloom(bloom) => bloom.count(),
        }
    }

    pub fn insert(&mut self, value: TagValueSet) {
        match &mut self.storage {
            TagValueSetStorage::Set(set) => {
                set.insert(value);
            }
            TagValueSetStorage::Bloom(bloom) => bloom.insert(&value),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::metric::TagValueSet;
    use crate::transforms::tag_cardinality_limit::config::Mode;

    #[test]
    fn test_accepted_tag_value_set_exact() {
        let mut accepted_tag_value_set = AcceptedTagValueSet::new(2, &Mode::Exact);

        assert!(!accepted_tag_value_set.contains(&TagValueSet::from(["value1".to_string()])));
        assert_eq!(accepted_tag_value_set.len(), 0);

        accepted_tag_value_set.insert(TagValueSet::from(["value1".to_string()]));
        assert_eq!(accepted_tag_value_set.len(), 1);
        assert!(accepted_tag_value_set.contains(&TagValueSet::from(["value1".to_string()])));

        accepted_tag_value_set.insert(TagValueSet::from(["value2".to_string()]));
        assert_eq!(accepted_tag_value_set.len(), 2);
        assert!(accepted_tag_value_set.contains(&TagValueSet::from(["value2".to_string()])));
    }

    #[test]
    fn test_accepted_tag_value_set_probabilistic() {
        let mut accepted_tag_value_set = AcceptedTagValueSet::new(2, &Mode::Exact);

        assert!(!accepted_tag_value_set.contains(&TagValueSet::from(["value1".to_string()])));
        assert_eq!(accepted_tag_value_set.len(), 0);

        accepted_tag_value_set.insert(TagValueSet::from(["value1".to_string()]));
        assert_eq!(accepted_tag_value_set.len(), 1);
        assert!(accepted_tag_value_set.contains(&TagValueSet::from(["value1".to_string()])));

        // Inserting the same value again should not increase the count.
        accepted_tag_value_set.insert(TagValueSet::from(["value1".to_string()]));
        assert_eq!(accepted_tag_value_set.len(), 1);
        assert!(accepted_tag_value_set.contains(&TagValueSet::from(["value1".to_string()])));

        accepted_tag_value_set.insert(TagValueSet::from(["value2".to_string()]));
        assert_eq!(accepted_tag_value_set.len(), 2);
        assert!(accepted_tag_value_set.contains(&TagValueSet::from(["value2".to_string()])));
    }
}
