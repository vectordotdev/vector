//! Template cache stub for NetFlow v5.
//!
//! NetFlow v5 uses a fixed record format and does not use templates. Full cache behavior for
//! NetFlow v9 / IPFIX will be added in a follow-up PR. `max_templates` / `template_timeout` in
//! config are reserved until then.

/// Minimal template cache for NetFlow v5: no storage; API preserved for the next protocol PR.
#[derive(Clone, Debug)]
pub struct TemplateCache;

impl TemplateCache {
    pub fn new(_max_size: usize) -> Self {
        Self
    }

    pub fn cleanup_expired(&self, _timeout_seconds: u64) {}
}
