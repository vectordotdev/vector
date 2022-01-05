use std::collections::{HashMap, HashSet};

use super::field;
use lookup::LookupBuf;
use value::{
    kind::{self, Collection},
    Kind,
};

/// The schema representation of the "output" produced by a component.
#[derive(Clone, Debug)]
pub struct Definition {
    /// The structure of the event.
    ///
    /// An event is _always_ a collection with fields (e.g. an "object" or "map").
    structure: kind::Collection<kind::Field>,

    /// Special purposes assigned to field within the structure.
    ///
    /// The value within this map points to a path inside the `structure`. It is an invalid state
    /// for there to be a purpose pointing to a non-existing path in the structure.
    purpose: HashMap<field::Purpose, LookupBuf>,

    /// A list of paths that are allowed to be missing.
    ///
    /// The key in this set points to a path inside the `structure`. It is an invalid state for
    /// there to be a key pointing to a non-existing path in the structure.
    optional: HashSet<LookupBuf>,
}

impl Definition {
    /// Create an "empty" output schema.
    ///
    /// This means no type information is known about the event.
    pub fn empty() -> Self {
        Self {
            structure: kind::Collection::any(),
            purpose: HashMap::default(),
            optional: HashSet::default(),
        }
    }

    /// Given structure, purposes, and optionals, create a new schema definition.
    ///
    /// # Errors
    ///
    /// Returns an error if any of the paths in `purpose` point to a non-existing path in
    /// `structure`.
    ///
    // TODO(Jean): Return proper error.
    pub fn from_parts(
        structure: kind::Collection<kind::Field>,
        purpose: HashMap<field::Purpose, LookupBuf>,
        optional: HashSet<LookupBuf>,
    ) -> Result<Self, ()> {
        let kind: Kind = structure.clone().into();

        for (_, path) in &purpose {
            if !kind.has_path(path.clone()) {
                return Err(());
            }
        }

        for path in &optional {
            if !kind.has_path(path.clone()) {
                return Err(());
            }
        }

        Ok(Self {
            structure,
            purpose,
            optional,
        })
    }

    /// Add type information for an event field.
    ///
    /// # Panics
    ///
    /// This function panics if the provided path is a root path (e.g. `.`).
    ///
    /// It also panics if the path points to a root-level array (e.g. `.[0]`).
    pub fn define_field(
        &mut self,
        path: impl Into<LookupBuf>,
        kind: Kind,
        purpose: Option<field::Purpose>,
    ) {
        let path = path.into();

        match path.get(0) {
            None => panic!("must not be a root path"),
            Some(segment) if segment.is_index() => panic!("must not start with an index"),
            _ => {}
        };

        let collection = kind
            .nest_at_path(&path.to_lookup())
            .into_object()
            .expect("always object");

        self.structure.merge(collection, true);

        if let Some(purpose) = purpose {
            self.purpose.insert(purpose, path);
        }
    }

    /// Add type information for an optional event field.
    ///
    /// # Panics
    ///
    /// This function panics if the provided path is a root path (e.g. `.`).
    pub fn define_optional_field(
        &mut self,
        path: impl Into<LookupBuf>,
        kind: Kind,
        purpose: Option<field::Purpose>,
    ) {
        let path = path.into();
        self.define_field(path.clone(), kind, purpose);
        self.optional.insert(path);
    }

    /// Set the kind for all undefined fields.
    pub fn define_other_fields(&mut self, kind: Kind) {
        self.structure.set_other(kind);
    }

    /// Get the type definition of the schema.
    pub fn to_kind(&self) -> value::Kind {
        self.structure.clone().into()
    }

    /// Get the collection .... TODO
    pub fn collection(&self) -> &Collection<kind::Field> {
        &self.structure
    }

    /// Get the [`Kind`] linked to a given purpose.
    pub fn kind_by_purpose(&self, purpose: &field::Purpose) -> Option<Kind> {
        self.purpose.get(purpose).cloned().and_then(|path| {
            let kind = Kind::object(self.structure.known().clone());
            kind.find_at_path(path)
        })
    }

    /// Get a list of field purposes and their path.
    pub fn purposes(&self) -> &HashMap<field::Purpose, LookupBuf> {
        &self.purpose
    }

    pub fn purpose_mut(&mut self) -> &mut HashMap<field::Purpose, LookupBuf> {
        &mut self.purpose
    }

    pub fn is_optional_field(&self, path: &LookupBuf) -> bool {
        self.optional.contains(path)
    }

    /// Merge `other` schema into `self`.
    ///
    /// If both schemas contain the same purpose key, then `other` key is used.
    pub fn merge(&mut self, other: Self) {
        // Optional Fields
        //
        // The merge strategy for optional fields is as follows:
        //
        // If the field is marked as optional in both definitions, _or_ if it's optional in one,
        // and unspecified in the other, then the field remains optional.
        //
        // If it's marked as "required" in either of the two definitions, then it becomes
        // a required field in the merged definition.
        //
        // Note that it is allowed to have required field nested under optional paths. For example,
        // `.foo` might be set as optional, but `.foo.bar` as required. In this case, it means that
        // the object at `.foo` is allowed to be missing, but if it's present, then it's required
        // to have a `bar` field.
        let mut optional = HashSet::default();
        for path in &self.optional {
            if other.is_optional_field(path) || !other.to_kind().has_path(path.clone()) {
                optional.insert(path.clone());
            }
        }
        for path in other.optional {
            if self.is_optional_field(&path) || !self.to_kind().has_path(path.clone()) {
                optional.insert(path);
            }
        }
        self.optional = optional;

        // Known fields
        //
        //
        self.structure.merge(other.structure, false);

        // Purpose
        self.purpose.extend(other.purpose);
    }
}

impl Default for Definition {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge() {
        // TODO: LOTS OF TESTING
        assert!(false)
    }
}
