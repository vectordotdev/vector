use std::collections::HashMap;

use super::field;
use lookup::LookupBuf;
use snafu::Snafu;
use value::{
    kind::{Collection, Field},
    Kind,
};

/// The input schema for a given component.
///
/// This schema defines the (semantic) fields a component expects to receive from its input
/// components.
#[derive(Debug, Clone)]
pub struct Requirement {
    fields: HashMap<field::Purpose, Kind>,

    /// A type requirement that the schema needs to match.
    ///
    /// While this can be used to define *exact* requirements on schema fields, it is primarily
    /// intended for sinks that want to encode an event to JSON, and require _all_ fields to be
    /// encodable to JSON.
    // FIXME(Jean): Need to actually validate this in the topology builder!
    structure: Collection<Field>,
}

impl Requirement {
    /// Create a new empty schema.
    ///
    /// An empty schema is the most "open" schema, in that there are no restrictions.
    pub fn empty() -> Self {
        Self {
            fields: HashMap::default(),
            structure: Collection::any(),
        }
    }

    pub fn purposes(&self) -> Vec<&field::Purpose> {
        self.fields.keys().collect()
    }

    /// Get a `Kind` object, containing all hard-required fields and their types.
    pub fn into_kind(&self) -> Kind {
        self.structure.clone().into()
    }

    // // TODO: tidy up naming
    // pub fn purposes_with_kinds(&self) -> &HashMap<field::Purpose, Kind> {
    //     &self.fields
    // }

    /// Add a restriction to the schema.
    pub fn require_field_purpose(&mut self, purpose: impl Into<field::Purpose>, kind: Kind) {
        self.fields.insert(purpose.into(), kind);
    }

    /// Set a hard requirement for an event field.
    ///
    /// # Panics
    ///
    /// Non-root fields are not supported at this time.
    pub fn require_field_kind(&mut self, path: LookupBuf, kind: Kind) {
        if path.is_root() {
            self.structure.set_other(kind);
            return;
        }

        // There is no reason why we can't support this, but there's no need yet, and it might
        // actually be something we want to actively discourage, so this panic serves as a reminder
        // that we probably want a brief discussion before enabling support for this.
        panic!("requiring exact field kind is currently unsupported")
    }

    /// Validate a given definition against the schema requirement.
    ///
    /// # Errors
    ///
    /// Returns a list of errors occured during validation:
    ///
    /// - `MissingPurpose`
    /// - `InvalidKind`
    pub fn validate(&self, definition: &super::Definition) -> Result<(), Vec<ValidationError>> {
        // 1. Check that all purposes defined in the requirement are present in the definition.
        //    - For all present purposes, make sure their types match.
        let mut errors = self
            .fields
            .iter()
            .filter_map(|(required_purpose, required_kind)| {
                definition
                    .kind_by_purpose(required_purpose)
                    .ok_or(ValidationError::MissingPurpose)
                    .and_then(|definition_kind| {
                        required_kind
                            .contains(&definition_kind)
                            .then(|| ())
                            .ok_or(ValidationError::InvalidKind)
                    })
                    .err()
            })
            .collect::<Vec<_>>();

        // 2. Check that the collection shape matches the shape of the definition.
        //    - Fields present in the definition but not in the requirement are allowed.
        //    - The reverse is not allowed.
        if !self.structure.contains(definition.collection()) {
            errors.push(ValidationError::InvalidShape);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl IntoIterator for Requirement {
    type Item = (field::Purpose, Kind);
    type IntoIter = std::collections::hash_map::IntoIter<field::Purpose, Kind>;

    fn into_iter(self) -> Self::IntoIter {
        self.fields.into_iter()
    }
}

#[derive(Debug, Snafu)]
pub enum ValidationError {
    #[snafu(display("invalid kind"))]
    InvalidKind,

    #[snafu(display("missing purpose"))]
    MissingPurpose,

    #[snafu(display("invalid shape"))]
    InvalidShape,
}
