use std::{
    collections::HashSet,
    fmt,
    hash::{BuildHasher, BuildHasherDefault},
};

use bloomy::BloomFilter;
use hash_hasher::HashedSet;
use seahash::SeaHasher;

use crate::{event::metric::TagValueSet, transforms::tag_cardinality_limit::config::Mode};

/// Container for storing the set of accepted values for a given tag key.
///
/// # Storage backend selection
///
/// | `Mode`               | Storage                         |
/// |----------------------|---------------------------------|
/// | `Exact`              | `HashSet<TagValueSet>`          |
/// | `ExactFingerprint`   | `HashSet<u64>` (fingerprints)   |
/// | `Probabilistic`      | `BloomFilter                    |

#[derive(Debug)]
pub struct AcceptedTagValueSet {
    storage: TagValueSetStorage,
}

enum TagValueSetStorage {
    Set(HashSet<TagValueSet>),
    Bloom(BloomFilterStorage),
    /// Stores 64-bit hash fingerprints of accepted tag values
    Fingerprint(FingerprintStorage),
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

struct FingerprintStorage {
    fps: HashedSet<u64>,
}

impl FingerprintStorage {
    fn new() -> Self {
        Self {
            fps: HashedSet::default(),
        }
    }

    /// Compute a 64-bit fingerprint of a tag value
    fn fingerprint(value: &TagValueSet) -> u64 {
        BuildHasherDefault::<SeaHasher>::default().hash_one(value)
    }

    fn insert(&mut self, value: &TagValueSet) {
        self.fps.insert(Self::fingerprint(value));
    }

    fn contains(&self, value: &TagValueSet) -> bool {
        self.fps.contains(&Self::fingerprint(value))
    }

    fn len(&self) -> usize {
        self.fps.len()
    }
}

impl fmt::Debug for TagValueSetStorage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TagValueSetStorage::Set(set) => write!(f, "Set({set:?})"),
            TagValueSetStorage::Bloom(_) => write!(f, "Bloom"),
            TagValueSetStorage::Fingerprint(_) => write!(f, "Fingerprint"),
        }
    }
}

impl AcceptedTagValueSet {
    /// Create a new `AcceptedTagValueSet` for the given mode.
    pub fn new(mode: &Mode) -> Self {
        let storage = match &mode {
            Mode::Exact => TagValueSetStorage::Set(HashSet::new()),
            Mode::ExactFingerprint => TagValueSetStorage::Fingerprint(FingerprintStorage::new()),
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
            TagValueSetStorage::Fingerprint(fp) => fp.contains(value),
        }
    }

    pub fn len(&self) -> usize {
        match &self.storage {
            TagValueSetStorage::Set(set) => set.len(),
            TagValueSetStorage::Bloom(bloom) => bloom.count(),
            TagValueSetStorage::Fingerprint(fp) => fp.len(),
        }
    }

    pub fn insert(&mut self, value: TagValueSet) {
        match &mut self.storage {
            TagValueSetStorage::Set(set) => {
                set.insert(value);
            }
            TagValueSetStorage::Bloom(bloom) => bloom.insert(&value),
            TagValueSetStorage::Fingerprint(fp) => fp.insert(&value),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::metric::TagValueSet,
        transforms::tag_cardinality_limit::config::{BloomFilterConfig, Mode},
    };

    #[test]
    fn test_accepted_tag_value_set_exact() {
        let mut accepted_tag_value_set = AcceptedTagValueSet::new(&Mode::Exact);

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
        // Previously this test mistakenly constructed Mode::Exact; fixed to use Probabilistic.
        let mut accepted_tag_value_set =
            AcceptedTagValueSet::new(&Mode::Probabilistic(BloomFilterConfig {
                cache_size_per_key: 5 * 1024,
            }));

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

    #[test]
    fn test_accepted_tag_value_set_fingerprint() {
        let mut set = AcceptedTagValueSet::new(&Mode::ExactFingerprint);

        assert!(!set.contains(&TagValueSet::from(["value1".to_string()])));
        assert_eq!(set.len(), 0);

        set.insert(TagValueSet::from(["value1".to_string()]));
        assert_eq!(set.len(), 1);
        assert!(set.contains(&TagValueSet::from(["value1".to_string()])));

        // Inserting the same value again must not increase the count.
        set.insert(TagValueSet::from(["value1".to_string()]));
        assert_eq!(set.len(), 1);

        set.insert(TagValueSet::from(["value2".to_string()]));
        assert_eq!(set.len(), 2);
        assert!(set.contains(&TagValueSet::from(["value2".to_string()])));

        // An un-inserted value must not appear to be contained.
        assert!(!set.contains(&TagValueSet::from(["value3".to_string()])));

        // Fingerprinting is deterministic, so a separate set must agree on membership.
        let mut set2 = AcceptedTagValueSet::new(&Mode::ExactFingerprint);
        set2.insert(TagValueSet::from(["value1".to_string()]));
        assert!(set2.contains(&TagValueSet::from(["value1".to_string()])));
        assert!(!set2.contains(&TagValueSet::from(["value3".to_string()])));
    }

    #[test]
    fn test_fingerprint_distribution_no_collisions() {
        // Empirically guards the "good distribution" claim: inserting many distinct values
        // must yield an equal number of distinct fingerprints. At 64 bits the birthday
        // collision probability for 100k values is ~2.7e-10, so any collision here would
        // indicate a badly-distributed hash rather than bad luck.
        let mut set = AcceptedTagValueSet::new(&Mode::ExactFingerprint);
        let n = 100_000;
        for i in 0..n {
            set.insert(TagValueSet::from([format!("tag-value-{i}")]));
        }
        assert_eq!(
            set.len(),
            n,
            "distinct values must produce distinct fingerprints"
        );
    }
}
