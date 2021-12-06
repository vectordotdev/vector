use std::collections::HashMap;

use super::field;
use value::Kind;

/// The input schema for a given component.
///
/// This schema defines the (semantic) fields a component expects to receive from its input
/// components.
#[derive(Debug)]
pub struct Input {
    fields: HashMap<field::Purpose, Kind>,
}

impl Input {
    /// Create a new empty schema.
    ///
    /// An empty schema is the most "open" schema, in that there are no restrictions.
    pub fn empty() -> Self {
        Self {
            fields: HashMap::default(),
        }
    }

    /// Add a restriction to the schema.
    pub fn require_field_purpose(&mut self, purpose: impl Into<field::Purpose>, kind: Kind) {
        self.fields.insert(purpose.into(), kind);
    }
}
