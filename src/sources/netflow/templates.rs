//! Shared cache object carried by the NetFlow source.
//!
//! NetFlow v5 does not exchange templates; this type stores no data but keeps the listener wiring
//! simple and leaves room for expansion without reshaping public structs.

/// Empty cache (NetFlow v5 has no template sets).
#[derive(Clone, Debug)]
pub struct TemplateCache;

impl TemplateCache {
    /// Builds an empty cache; `max_size` is accepted for configuration symmetry only.
    pub fn new(_max_size: usize) -> Self {
        Self
    }

    /// Cache maintenance hook invoked by the worker loop (no-op for NetFlow v5).
    pub fn cleanup_expired(&self, _timeout_seconds: u64) {}
}
