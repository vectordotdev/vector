// A significant portion of this code -- most all types in gen.rs, schema.rs, and visit.rs -- are
// copied from the `schemars` crate. The license for `schemars` is included in `LICENSE-schemars`,
// pursuant to the listed conditions in the license.

mod gen;
mod schema;
pub mod visit;

pub type Map<K, V> = indexmap::IndexMap<K, V>;
pub type Set<V> = std::collections::BTreeSet<V>;

pub use self::gen::{SchemaGenerator, SchemaSettings};
pub use self::schema::*;
