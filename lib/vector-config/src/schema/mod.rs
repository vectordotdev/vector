// Public re-export of all of the core schema generation types that live in `vector-config-common`.
pub use vector_config_common::schema::*;

// Helpers for reducing boilerplate i.e. generating type-specific schemas with default values, and
// so on.
mod helpers;
pub use self::helpers::*;

pub mod visitors;

pub mod parser;
