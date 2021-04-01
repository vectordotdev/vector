/// [`VectorRegistry`] is a vendored version of [`metrics_util::Registry`].
///
/// We are removing the generational wrappers that upstream added, as they
/// might've been the cause of the performance issues on the multi-core systems
/// under high paralellism.
///
/// The suspicion is that the atomics usage in the generational somehow causes
/// permanent cache invalidation starvation at some scenarios - however, it's
/// based on the empiric observations, and we currently don't have
/// a comprehensive mental model to back up this behaviour.
/// It was decided to just eliminate the generationals - for now.
/// Maybe in the long term too - we don't need them, so why pay the price?
/// They're not zero-cost.
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hash};
use std::sync::{Arc, Mutex};
use twox_hash::XxHash64;

type Map<K, H> = HashMap<K, H, BuildHasherDefault<XxHash64>>;

#[derive(Debug)]
pub(crate) struct VectorRegistry<K, H>
where
    K: Eq + Hash + Clone + 'static,
    H: 'static,
{
    pub map: Arc<Mutex<Map<K, H>>>,
}

impl<K, H> Default for VectorRegistry<K, H>
where
    K: Eq + Hash + Clone + 'static,
    H: 'static,
{
    fn default() -> Self {
        Self {
            map: Arc::new(Mutex::new(HashMap::default())),
        }
    }
}

impl<K, H> Clone for VectorRegistry<K, H>
where
    K: Eq + Hash + Clone + 'static,
    H: 'static,
{
    fn clone(&self) -> Self {
        Self {
            map: Arc::clone(&self.map),
        }
    }
}
