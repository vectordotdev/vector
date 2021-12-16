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

    /// Create a collection kind of which the encapsulated values can be any kind.
    #[must_use]
    pub fn any() -> Self {
        Self {
            known: BTreeMap::default(),
            other: Other::Any,
        }
    }

    /// Create a collection kind of which the encapsulated values can be any JSON-compatible kind.
    #[must_use]
    pub fn json() -> Self {
        Self {
            known: BTreeMap::default(),
            other: Other::Exact(Box::new(Kind::json())),
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

impl Index {
    #[must_use]
    pub fn take(self) -> usize {
        self.0
    }
}

impl From<usize> for Index {
    fn from(index: usize) -> Self {
        Self(index)
    }
}

/// A `field` type that can be used in `Collection<Field>`
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct Field(Cow<'static, str>);

impl Field {
    #[must_use]
    pub fn take(self) -> Cow<'static, str> {
        self.0
    }
}

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

impl From<lookup::FieldBuf> for Field {
    fn from(field: lookup::FieldBuf) -> Self {
        Self(field.to_string().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge() {
        // (any) & (any) = (any)
        let mut left = Collection::<()>::any();
        let right = Collection::<()>::any();
        left.merge(right);
        assert_eq!(left, Collection::<()>::any());

        // merge two separate known fields
        let mut left: Collection<&str> = BTreeMap::from([("foo", Kind::null())]).into();
        let right: Collection<&str> = BTreeMap::from([("bar", Kind::bytes())]).into();
        left.merge(right);
        assert_eq!(left, {
            let mut want: Collection<&str> =
                BTreeMap::from([("foo", Kind::null()), ("bar", Kind::bytes())]).into();
            want.set_other(Kind::any());
            want
        });

        // merge similarly named known field
        let mut left: Collection<&str> = BTreeMap::from([("foo", Kind::null())]).into();
        let right: Collection<&str> = BTreeMap::from([("foo", Kind::bytes())]).into();
        left.merge(right);
        assert_eq!(left, {
            let mut want: Collection<&str> = BTreeMap::from([("foo", {
                let mut kind = Kind::null();
                kind.add_bytes();
                kind
            })])
            .into();
            want.set_other(Kind::any());
            want
        });

        // merge nested fields
        let inner = BTreeMap::from([("bar".into(), Kind::integer())]);
        let mut left: Collection<&str> = BTreeMap::from([("foo", Kind::object(inner))]).into();
        let right: Collection<&str> = BTreeMap::from([("bar", Kind::bytes())]).into();
        left.merge(right);
        assert_eq!(left, {
            let inner = BTreeMap::from([("bar".into(), Kind::integer())]);
            let mut want: Collection<&str> =
                BTreeMap::from([("foo", Kind::object(inner)), ("bar", Kind::bytes())]).into();
            want.set_other(Kind::any());
            want
        });

        // merge "other" fields
        let mut left: Collection<&str> = BTreeMap::from([("foo", Kind::boolean())]).into();
        left.set_other(Kind::integer());
        let mut right: Collection<&str> = BTreeMap::from([("bar", Kind::bytes())]).into();
        right.set_other(Kind::timestamp());
        left.merge(right);
        assert_eq!(left, {
            let mut want: Collection<&str> =
                BTreeMap::from([("foo", Kind::boolean()), ("bar", Kind::bytes())]).into();
            want.set_other({
                let mut kind = Kind::integer();
                kind.add_timestamp();
                kind
            });
            want
        });
    }

    #[test]
    fn test_any() {
        let v = Collection::<()>::any();

        assert!(v.known().is_empty());
        assert!(v.other().is_any());
    }

    #[test]
    fn test_is_any() {
        let v = Collection::<()>::any();
        assert!(v.is_any());

        let v: Collection<&str> = BTreeMap::from([("foo", Kind::any())]).into();
        assert!(v.is_any());

        let mut v: Collection<&str> = BTreeMap::from([("foo", Kind::any())]).into();
        v.set_other(Kind::boolean());
        assert!(!v.is_any());

        let mut v: Collection<&str> = BTreeMap::from([("foo", Kind::integer())]).into();
        v.set_other(Kind::any());
        assert!(!v.is_any());

        let mut v: Collection<&str> =
            BTreeMap::from([("foo", Kind::any()), ("bar", Kind::boolean())]).into();
        v.set_other(Kind::any());
        assert!(!v.is_any());
    }

    #[test]
    fn test_set_other() {
        let mut v = Collection::<()>::any();
        assert_eq!(v.other, Other::Any);

        v.set_other(Kind::integer());
        assert_eq!(v.other, Other::Exact(Box::new(Kind::integer())));
    }
}
