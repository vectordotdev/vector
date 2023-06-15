// A significant portion of this code -- most all types in gen.rs, schema.rs, and visit.rs -- are
// copied from the `schemars` crate. The license for `schemars` is included in `LICENSE-schemars`,
// pursuant to the listed conditions in the license.

mod gen;
mod json_schema;
pub mod visit;

pub(crate) const DEFINITIONS_PREFIX: &str = "#/definitions/";

// We have chosen the `BTree*` types here instead of hash tables to provide for a consistent
// ordering of the output elements between runs and changes to the configuration.
pub type Map<K, V> = std::collections::BTreeMap<K, V>;
pub type Set<V> = std::collections::BTreeSet<V>;

pub use self::gen::{SchemaGenerator, SchemaSettings};
pub use self::json_schema::*;
