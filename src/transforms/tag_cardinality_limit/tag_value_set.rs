use crate::event::metric::TagValueSet;
use crate::transforms::tag_cardinality_limit::config::Mode;
use bloom::{BloomFilter, ASMS};
use std::collections::HashSet;
use std::fmt;

/// Container for storing the set of accepted values for a given tag key.
#[derive(Debug)]
pub struct AcceptedTagValueSet {
    storage: TagValueSetStorage,
    num_elements: usize,
}

enum TagValueSetStorage {
    Set(HashSet<TagValueSet>),
    Bloom(BloomFilter),
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
    pub fn new(value_limit: u32, mode: &Mode) -> Self {
        let storage = match &mode {
            Mode::Exact => TagValueSetStorage::Set(HashSet::with_capacity(value_limit as usize)),
            Mode::Probabilistic(config) => {
                let num_bits = config.cache_size_per_key / 8; // Convert bytes to bits
                let num_hashes = bloom::optimal_num_hashes(num_bits, value_limit);
                TagValueSetStorage::Bloom(BloomFilter::with_size(num_bits, num_hashes))
            }
        };
        Self {
            storage,
            num_elements: 0,
        }
    }

    pub fn contains(&self, value: &TagValueSet) -> bool {
        match &self.storage {
            TagValueSetStorage::Set(set) => set.contains(value),
            TagValueSetStorage::Bloom(bloom) => bloom.contains(&value),
        }
    }

    pub const fn len(&self) -> usize {
        self.num_elements
    }

    pub fn insert(&mut self, value: TagValueSet) -> bool {
        let inserted = match &mut self.storage {
            TagValueSetStorage::Set(set) => set.insert(value),
            TagValueSetStorage::Bloom(bloom) => bloom.insert(&value),
        };
        if inserted {
            self.num_elements += 1
        }
        inserted
    }
}
