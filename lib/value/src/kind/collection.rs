use super::Kind;
use std::{borrow::Cow, collections::BTreeMap};

/// The kinds of a collection (array or object).
///
/// A collection contains one or more kinds for known positions within the object (indices or
/// fields), and contains a global "other" state that applies to all unknown indices or fields.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Collection<T: Ord> {
    known: BTreeMap<T, Kind>,

    /// The kind of other non-known fields.
    ///
    /// For example, an array collection might be known to have an "integer" state at the 0th
    /// index, but it has an unknown length. It is however known that whatever length the array
    /// has, its values can only be integers or floats, so the `other` state is set to those two.
    other: Other,
}

/// An internal wrapper type to avoid infinite recursion when a collection's `other` field is set
/// to `any`.
#[derive(Debug, Clone, Eq, PartialEq)]
enum Other {
    Any,
    Exact(Box<Kind>),
}

impl<T: Ord> Collection<T> {
    /// Merge `other` collection into `self`.
    ///
    /// For the "other" fields, this means if any of the two collections have "any" other field
    /// kinds, then the merged collection also has "any" other fields.
    ///
    /// For "known" fields, if both collections contain the same field, their kinds are merged, if
    /// `self` has a field that `other` does not, then that field is kept, and if `other` has
    /// a field that `self` has not, the field is added to `self`.
    pub fn merge(&mut self, mut other: Self) {
        // Set the "other" field to a merged exact value, or "any".
        match (&mut self.other, other.other) {
            (Other::Exact(this), Other::Exact(other)) => this.merge(*other),
            _ => self.other = Other::Any,
        }

        // Merge fields that are known for both collections.
        for (key, state) in &mut self.known {
            if let Some(other_state) = other.known.remove(key) {
                state.merge(other_state);
            }
        }

        // Append known fields in `other` that are unknown to `self`.
        self.known.append(&mut other.known);
    }
}

impl<T: Ord> Collection<T> {
    /// Create a collection kind of which the encapsulated values can be any kind.
    #[must_use]
    pub fn any() -> Self {
        Self {
            known: BTreeMap::default(),
            other: Other::Any,
        }
    }

    /// Check if the collection fields can be of any kind.
    ///
    /// This returns `false` if at least _one_ field kind is known.
    #[must_use]
    pub fn is_any(&self) -> bool {
        self.known.iter().all(|(_, k)| k.is_any()) && matches!(self.other, Other::Any)
    }

    /// Get the "known" field value kinds.
    #[must_use]
    pub fn known(&self) -> &BTreeMap<T, Kind> {
        &self.known
    }

    /// Get the "other" field value kind.
    #[must_use]
    pub fn other(&self) -> Cow<'_, Kind> {
        match &self.other {
            Other::Any => Cow::Owned(Kind::any()),
            Other::Exact(kind) => Cow::Borrowed(kind),
        }
    }

    /// Set the "other" field values to the given kind.
    pub fn set_other(&mut self, kind: Kind) {
        if kind.is_any() {
            self.other = Other::Any;
            return;
        }

        self.other = Other::Exact(Box::new(kind));
    }
}

impl<T: Ord> From<BTreeMap<T, Kind>> for Collection<T> {
    fn from(known: BTreeMap<T, Kind>) -> Self {
        Self {
            known,
            other: Other::Any,
        }
    }
}

/// An `index` type that can be used in `Collection<Index>`
#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
pub struct Index(usize);

impl From<usize> for Index {
    fn from(index: usize) -> Self {
        Self(index)
    }
}

/// A `field` type that can be used in `Collection<Field>`
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct Field(Cow<'static, str>);

impl From<&'static str> for Field {
    fn from(field: &'static str) -> Self {
        Self(field.into())
    }
}

impl From<String> for Field {
    fn from(field: String) -> Self {
        Self(field.into())
    }
}
