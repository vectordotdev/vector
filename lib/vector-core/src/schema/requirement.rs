use std::collections::BTreeMap;

use lookup::LookupBuf;
use value::{
    kind::{Collection, Field, Unknown},
    Kind,
};

/// The input schema for a given component.
///
/// This schema defines the (semantic) fields a component expects to receive from its input
/// components.
#[derive(Debug, Clone, PartialEq)]
pub struct Requirement {
    /// The collection of fields and their types required to be present in the event.
    ///
    /// While this can be used to define *exact* requirements on schema fields, it is primarily
    /// intended for sinks that want to add a type requirement to _all_ fields in the event (e.g.
    /// JSON encoding).
    collection: Collection<Field>,

    /// Semantic meaning required to exists for a given event.
    meaning: BTreeMap<&'static str, Kind>,
}

impl Requirement {
    /// Create a new empty schema.
    ///
    /// An empty schema is the most "open" schema, in that there are no restrictions.
    pub fn empty() -> Self {
        Self {
            collection: Collection::any(),
            meaning: BTreeMap::default(),
        }
    }

    /// Check if the requirement is "empty", meaning:
    ///
    /// 1. There are no required fields defined.
    /// 2. The unknown fields are set to "any".
    /// 3. There are no required meanings defined.
    pub fn is_empty(&self) -> bool {
        self.collection.known().is_empty()
            && self.collection.unknown().map_or(false, Unknown::is_any)
            && self.meaning.is_empty()
    }

    /// Add a restriction to the schema.
    pub fn require_meaning(mut self, meaning: &'static str, kind: Kind) -> Self {
        self.meaning.insert(meaning, kind);
        self
    }

    /// Set a hard requirement for an event field.
    ///
    /// # Panics
    ///
    /// Non-root fields are not supported at this time.
    pub fn require_field(mut self, path: &LookupBuf, kind: Kind) -> Self {
        // There is no reason why we can't support this, but there's no need yet, and it might
        // actually be something we want to actively discourage, so this panic serves as a reminder
        // that we probably want a brief discussion before enabling support for this.
        if !path.is_root() {
            panic!("requiring exact field kind is currently unsupported")
        }

        self.collection.set_unknown(kind);
        self
    }
}
