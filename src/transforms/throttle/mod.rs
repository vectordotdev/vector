pub mod config;
pub mod rate_limiter;
pub mod transform;

/// Output port name for rate-limited events when `reroute_dropped` is enabled.
pub const DROPPED: &str = "dropped";
