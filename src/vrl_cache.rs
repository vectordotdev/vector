//! Functionality to handle VRL caches.
use vector_lib::configurable::configurable_component;

/// Configurable VRL caches.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct VrlCaches {
    /// TTL (time-to-live), used to limit lifetime of data stored in cache.
    /// When TTL expires, data behind a specific key in cache is removed.
    /// TTL is restarted when using the key.
    #[serde(default = "default_ttl")]
    ttl: u64,
    /// Scan interval for updating TTL of keys in seconds. This is provided
    /// as an optimization, to ensure that TTL is updated, but without doing
    /// too many cache scans.
    #[serde(default = "default_scan_interval")]
    scan_interval: u64,
}

impl Default for VrlCaches {
    fn default() -> Self {
        Self {
            ttl: default_ttl(),
            scan_interval: default_scan_interval(),
        }
    }
}

const fn default_ttl() -> u64 {
    600
}

const fn default_scan_interval() -> u64 {
    30
}
