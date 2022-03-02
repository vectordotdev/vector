use std::collections::BTreeMap;

use value::Kind;

/// The input schema for a given component.
///
/// This schema defines the (semantic) fields a component expects to receive from its input
/// components.
#[derive(Debug, Clone, PartialEq)]
pub struct Requirement {
    /// Semantic meanings confingured for this requirement.
    meaning: BTreeMap<&'static str, SemanticMeaning>,
}

/// The semantic meaning of an event.
#[derive(Debug, Clone, PartialEq)]
struct SemanticMeaning {
    /// The type required by this semantic meaning.
    kind: Kind,

    /// Whether the meaning is optional.
    ///
    /// If a meaning is optional, the sink must not error when the meaning is not defined in the
    /// provided `Definition`, but it *must* error if it is defined, but its type does not meet the
    /// requirement.
    optional: bool,
}

impl SemanticMeaning {
    fn new(kind: Kind, optional: bool) -> Self {
        Self { kind, optional }
    }
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
    pub fn required_meaning(mut self, meaning: &'static str, kind: Kind) -> Self {
        self.insert_meaning(meaning, kind, true);
        self
    }

    /// Add an optional restriction to the schema.
    ///
    /// This differs from `required_meaning` in that it is valid for the event to not have the
    /// specified meaning defined, but invalid for that meaning to be defined, but its [`Kind`] not
    /// matching the configured expectation.
    pub fn optional_meaning(mut self, meaning: &'static str, kind: Kind) -> Self {
        self.insert_meaning(meaning, kind, false);
        self
    }

    fn insert_meaning(&mut self, identifier: &'static str, kind: Kind, optional: bool) {
        let meaning = SemanticMeaning { kind, optional };
        self.meaning.insert(identifier, meaning);
    }
}
