//! Field parsing stub for NetFlow v5.
//!
//! NetFlow v5 uses a fixed record layout; template-based field resolution is reserved for a
//! future NetFlow v9 / IPFIX change. `FieldParser` exists so `NetflowV5Parser::new` stays stable.

use crate::sources::netflow::config::NetflowConfig;

/// Placeholder for future template-based field parsing (v9/IPFIX).
#[derive(Clone, Copy, Debug, Default)]
pub struct FieldParser;

impl FieldParser {
    /// Creates a parser; `config` is unused for v5-only builds but keeps the API stable.
    pub fn new(_config: &NetflowConfig) -> Self {
        Self
    }
}
