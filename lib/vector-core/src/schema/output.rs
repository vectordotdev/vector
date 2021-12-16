use std::collections::{HashMap, HashSet};

use super::field;
use lookup::LookupBuf;
use value::{kind, Kind};

/// The schema representation of the "output" produced by a component.
#[derive(Clone, Debug)]
pub struct Output {
    kind: value::Kind,
    purpose: HashMap<field::Purpose, LookupBuf>,

    /// A list of paths that are allowed to be missing.
    optional: HashSet<LookupBuf>,
}

impl Output {
    /// Create an "empty" output schema.
    ///
    /// This means no type information is known about the event.
    pub fn empty() -> Self {
        Self {
            kind: Kind::object(kind::Collection::any()),
            purpose: HashMap::default(),
            optional: HashSet::default(),
        }
    }

    /// Given kinds and purposes, create a new output schema.
    ///
    /// # Panics
    ///
    /// This function panics if the provided `kind` is not of an `object` type.
    pub fn from_parts(
        kind: value::Kind,
        purpose: HashMap<field::Purpose, LookupBuf>,
        optional: HashSet<LookupBuf>,
    ) -> Self {
        assert!(kind.is_object());

        Self {
            kind,
            purpose,
            optional,
        }
    }

    /// Add type information for an event field.
    ///
    /// # Panics
    ///
    /// This function panics if the provided path is a root path (e.g. `.`).
    pub fn define_field(
        &mut self,
        path: impl Into<LookupBuf>,
        kind: Kind,
        purpose: Option<field::Purpose>,
    ) {
        let path = path.into();
        assert!(!path.is_root());

        let kind = kind.nest_at_path(&path.to_lookup());
        self.kind.merge(kind);

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
        self.kind
            .as_object_mut()
            .expect("must always be an object")
            .set_other(kind);
    }

    /// Get the type definition of the schema.
    pub fn kind(&self) -> &value::Kind {
        &self.kind
    }

    /// Get a list of field purposes and their path.
    pub fn purpose(&self) -> &HashMap<field::Purpose, LookupBuf> {
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
    /// If both schemas contain the same purpose key, then `other` key is used. In the future, we
    /// might update this to return an error, and prevent Vector from booting.
    pub fn merge(&mut self, other: Self) {
        // Optional Fields
        //
        // The merge strategy for optional fields is as follows:
        //
        // 1. For any optional field in `Self`, keep it only if the field is also optional in
        //    `other` _or_ if the field is unspecified in `other`.
        // 2. Do the same, but the inverse for `other`, comparing against `self.
        // 3. Add the resulting fields from the above two steps as the new optional fields.
        //
        // Note that it is allowed to have required field nested under optional paths. For example,
        // `.foo` might be set as optional, but `.foo.bar` as required. In this case, it means that
        // the object at `.foo` is allowed to be missing, but if it's present, then it's required
        // to have a `bar` field.
        let mut optional = HashSet::default();
        for path in &self.optional {
            if other.is_optional_field(path) || !other.kind.has_path(path.clone()) {
                optional.insert(path.clone());
            }
        }
        for path in other.optional {
            if self.is_optional_field(&path) || !self.kind.has_path(path.clone()) {
                optional.insert(path);
            }
        }
        self.optional = optional;

        // Kind
        self.kind.merge(other.kind);

        // Purpose
        self.purpose.extend(other.purpose);
    }
}

impl Default for Output {
    fn default() -> Self {
        Self::empty()
    }
}
