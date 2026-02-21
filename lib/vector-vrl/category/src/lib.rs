use strum::AsRefStr;

/// Category classification for Vector-specific VRL functions.
///
/// This enum complements the categories defined in the VRL stdlib,
/// providing Vector-specific categories for enrichment, metrics, and event functions.
#[derive(Debug, Clone, Copy, AsRefStr)]
#[strum(serialize_all = "PascalCase")]
pub enum Category {
    /// Enrichment table operations
    Enrichment,
    /// Event metadata and secret management
    Event,
    /// Internal Vector metrics operations
    Metrics,
}
