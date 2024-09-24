use vector_lib::configurable::configurable_component;

use crate::vrl_cache::VrlCaches;

/// Fully resolved VRL caches component.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct VrlCacheOuter {
    #[serde(flatten)]
    pub inner: VrlCaches,
}

impl VrlCacheOuter {
    pub fn new<I: Into<VrlCaches>>(inner: I) -> Self {
        Self {
            inner: inner.into(),
        }
    }
}
