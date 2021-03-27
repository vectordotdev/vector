use serde::{Deserialize, Serialize};
use shared::EventDataEq;
use std::sync::atomic::{AtomicU32, Ordering};

#[cfg(test)]
lazy_static::lazy_static! {
    // This is a non-zero sample value for `EventMetadata::generation`
    // that will be constant across all test data, causing event
    // metadata to compare equal when using it, but different on each
    // run to prevent hard-coding in tests to work. Additionally, it
    // will be large enough that no test will ever generate enough
    // events to reach it normally, preventing accidental collisions.
    static ref TEST_GENERATION: u32 = {
        use rand::distributions::{Distribution, Uniform};
        Uniform::from(2^30..=u32::MAX).sample(&mut rand::thread_rng())
    };
}

static GENERATION: AtomicU32 = AtomicU32::new(1);

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EventMetadata {
    /// Simple generation counter, different for every new event.
    generation: u32,
}

impl Default for EventMetadata {
    fn default() -> Self {
        Self {
            // Ordering can be relaxed because this isn't a sequence
            // point for other operations.
            generation: GENERATION.fetch_add(1, Ordering::Relaxed),
        }
    }
}

impl EventMetadata {
    #[cfg(test)]
    pub fn test_default() -> Self {
        Self {
            generation: *TEST_GENERATION,
        }
    }

    pub fn merge(&mut self, other: &Self) {
        self.generation = self.generation.min(other.generation)
    }
}

impl EventDataEq for EventMetadata {
    fn event_data_eq(&self, _other: &Self) -> bool {
        true
    }
}
