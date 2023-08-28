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
    Bloom(BloomFilter<TagValueSet>),
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
                let num_bits = config.cache_size_per_key / 8; // Convert bytes to bits
                TagValueSetStorage::Bloom(BloomFilter::with_size(num_bits))
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
