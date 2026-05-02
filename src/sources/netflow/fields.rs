//! Field decoding helpers for the NetFlow source.
//!
//! NetFlow v5 records use a fixed layout; the parser reads bytes directly. `FieldParser` is a
//! zero-sized handle passed into the v5 parser constructor so call sites stay uniform.

use crate::sources::netflow::config::NetflowConfig;

/// Zero-sized parser handle (no runtime state for NetFlow v5).
#[derive(Clone, Copy, Debug, Default)]
pub struct FieldParser;

impl FieldParser {
    /// Constructs a handle; `config` is currently unused.
    pub fn new(_config: &NetflowConfig) -> Self {
        Self
    }
}
