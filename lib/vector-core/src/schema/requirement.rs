use std::collections::BTreeMap;

use value::Kind;

/// The input schema for a given component.
///
/// This schema defines the (semantic) fields a component expects to receive from its input
/// components.
#[derive(Debug, Clone, PartialEq)]
pub struct Requirement {
    /// Semantic meaning required to exist for a given event.
    meaning: BTreeMap<&'static str, Kind>,
}

impl Requirement {
    /// Create a new empty schema.
    ///
    /// An empty schema is the most "open" schema, in that there are no restrictions.
    pub fn empty() -> Self {
        Self {
            meaning: BTreeMap::default(),
        }
    }

    /// Check if the requirement is "empty", meaning:
    ///
    /// 1. There are no required fields defined.
    /// 2. The unknown fields are set to "any".
    /// 3. There are no required meanings defined.
    pub fn is_empty(&self) -> bool {
        self.meaning.is_empty()
    }

    /// Add a restriction to the schema.
    pub fn require_meaning(mut self, meaning: &'static str, kind: Kind) -> Self {
        self.meaning.insert(meaning, kind);
        self
    }
}
