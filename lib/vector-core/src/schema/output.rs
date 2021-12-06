use std::collections::HashMap;

use super::field;
use lookup::LookupBuf;
use value::{kind, Kind};

/// The schema representation of the "output" produced by a component.
#[derive(Clone, Debug)]
pub struct Output {
    kind: value::Kind,
    purpose: HashMap<field::Purpose, LookupBuf>,
}

impl Output {
    /// Create an "empty" output schema.
    ///
    /// This means no type information is known about the event.
    pub fn empty() -> Self {
        Self {
            kind: Kind::object(kind::Collection::any()),
            purpose: HashMap::default(),
        }
    }

    /// Given kinds and purposes, create a new output schema.
    pub fn from_parts(kind: value::Kind, purpose: HashMap<field::Purpose, LookupBuf>) -> Self {
        Self { kind, purpose }
    }

    /// Add type information for an event field.
    pub fn define_field(
        &mut self,
        path: impl Into<LookupBuf>,
        kind: Kind,
        purpose: Option<field::Purpose>,
    ) {
        let path = path.into();
        let kind = kind.nest_at_path(&path.to_lookup());
        self.kind.merge(kind);

        if let Some(purpose) = purpose {
            self.purpose.insert(purpose, path);
        }
    }

    pub fn kind(&self) -> &value::Kind {
        &self.kind
    }

    pub fn purpose(&self) -> &HashMap<field::Purpose, LookupBuf> {
        &self.purpose
    }
}

impl Default for Output {
    fn default() -> Self {
        Self::empty()
    }
}
