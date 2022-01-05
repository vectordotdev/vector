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

impl<T: Ord> Collection<T> {
    /// Merge `other` collection into `self`.
    ///
    /// For the "other" fields, this means if any of the two collections have "any" other field
    /// kinds, then the merged collection also has "any" other fields.
    ///
    /// For "known" fields, if both collections contain the same field, their kinds are merged, if
    /// `self` has a field that `other` does not, then that field is kept, and if `other` has
    /// a field that `self` has not, the field is added to `self`.
    ///
    /// if `overwrite_known` is `true`, then any existing known fields are overwritten.
    pub fn merge(&mut self, mut other: Self, overwrite_known: bool) {
        // Set the "other" field to a merged exact value, or "any".
        self.other.merge(other.other);

        // Merge fields that are known for both collections.
        for (key, this_kind) in &mut self.known {
            if let Some(other_kind) = other.known.remove(key) {
                if overwrite_known {
                    this_kind.merge_collections(other_kind);
                } else {
                    // TODO(Jean): ...
                    let strategy = super::MergeStrategy {
                        known_fields: super::KnownFieldMergeStrategy::Merge,
                        collections: MergeStrategy::Deep,
                    };

                    this_kind.merge(other_kind, strategy);
                }
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
            other: Other::any(),
        }
    }

    /// Create a collection kind of which the encapsulated values can be any JSON-compatible kind.
    #[must_use]
    pub fn json() -> Self {
        Self {
            known: BTreeMap::default(),
            other: Other::json(),
        }
    }

    /// Check if the collection fields can be of any kind.
    ///
    /// This returns `false` if at least _one_ field kind is known.
    #[must_use]
    pub fn is_any(&self) -> bool {
        self.known.iter().all(|(_, k)| k.is_any()) && self.other.is_any()
    }

    /// Get the "known" field value kinds.
    #[must_use]
    pub fn known(&self) -> &BTreeMap<T, Kind> {
        &self.known
    }

    /// Get the "other" field value kind.
    #[must_use]
    pub fn other(&self) -> Kind {
        self.other.clone().into_kind()
    }

    /// Set the "other" field values to the given kind.
    pub fn set_other(&mut self, kind: Kind) {
        self.other = kind.into();
    }

    /// Check if "self" contains "other".
    ///
    /// This returns true if [`Other`] matches exactly, and known fields in `self` are present in
    /// `other` (meaning that fields present in `other` but not in `self` are allowed).
    ///
    /// Additionally, required fields must match their respective `Kind`.
    ///
    /// TODO(Jean): We'll want to know which errors triggered `false` here, so we likely want this
    /// to return `Result<(), Vec<Error>>` instead.
    #[must_use]
    pub fn contains(&self, other: &Self) -> bool {
        // If we accept any collection, then whatever we're checking against will always be valid.
        if self.is_any() {
            return true;
        }

        // If we have no known fields, we only need to check if the "other" fields are contained.
        //
        // It's okay for "other" (the variable) to have known fields in this case, we don't care
        // about them.
        if self.known().is_empty() {
            return self.other().contains(&other.other());
        }

        // Finally, if we do have known fields, we need to compare them against the known fields of
        // `other`.
        let contained_knowns =
            self.known
                .iter()
                .all(|(key, this_kind)| match other.known().get(key) {
                    Some(other_kind) => this_kind.contains(other_kind),
                    None => false,
                });

        // And also compare others.
        let contained_others = self.other().contains(&other.other());

        contained_knowns && contained_others
    }
}

impl<T: Ord> From<BTreeMap<T, Kind>> for Collection<T> {
    fn from(known: BTreeMap<T, Kind>) -> Self {
        Self {
            known,
            other: Other::any(),
        }
    }
}

/// An internal wrapper type to avoid infinite recursion when a collection's `other` field contains
/// a collection.
#[derive(Debug, Clone, Eq, PartialEq)]
struct Other {
    any: bool,
    primitives: Option<Box<Kind>>,

    // NOTE: we don't need to support nested objects, because `Other` is only used for "unknown"
    // fields. So it only applies to a single level. If we wanted to set a nested field, we would
    // have a "known" field (e.g. `foo`) and then have that contain `Other`.
    object: bool,
    array: bool,
}

impl Other {
    fn any() -> Self {
        Self {
            any: true,
            primitives: None,
            object: false,
            array: false,
        }
    }

    fn json() -> Self {
        Self {
            any: false,
            primitives: Some(Box::new(Kind::primitive())),
            object: true,
            array: true,
        }
    }

    fn is_any(&self) -> bool {
        self.any
    }

    fn into_kind(self) -> Kind {
        if self.any {
            return Kind::any();
        }

        let mut kind = self
            .primitives
            .as_ref()
            .map_or_else(Kind::empty, |v| *v.clone());

        if self.object {
            kind.add_object(BTreeMap::default());
        }

        if self.array {
            kind.add_array(BTreeMap::default());
        }

        if kind.is_empty() {
            panic!("invalid `other` state")
        }

        kind
    }

    fn merge(&mut self, other: Self) {
        if self.is_any() || other.is_any() {
            *self = Self::any();
            return;
        }

        let primitives = match (self.primitives.as_mut(), other.primitives) {
            (None, None) => None,
            (v @ Some(_), None) => v.cloned(),
            (None, v @ Some(_)) => v,
            (Some(this), Some(other)) => {
                // TODO(Jean): correct merge strategy
                this.merge(*other, super::MergeStrategy::default());
                Some(this.clone())
            }
        };

        *self = Self {
            any: false,
            primitives,
            object: self.object || other.object,
            array: self.array || other.array,
        };
    }
}

impl From<Kind> for Other {
    fn from(kind: Kind) -> Self {
        if kind.is_any() {
            return Other::any();
        }

        let array = kind.is_array();
        let object = kind.is_object();

        Self {
            any: false,
            primitives: kind.to_primitive().map(Box::new),
            array,
            object,
        }
    }
}

impl From<Other> for Kind {
    fn from(other: Other) -> Self {
        if other.any {
            return Kind::any();
        }

        let mut kind = other
            .primitives
            .as_ref()
            .map_or_else(Kind::empty, |v| *v.clone());

        if other.object {
            kind.add_object(BTreeMap::default());
        }

        if other.array {
            kind.add_array(BTreeMap::default());
        }

        if kind.is_empty() {
            panic!("invalid `other` state")
        }

        kind
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

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum MergeStrategy {
    /// Collection types are merged recursively.
    ///
    /// That is, given:
    ///
    /// ```json,ignore
    /// { "foo": { "bar": true, "baz": { "qux": true } } }
    /// ```
    ///
    /// merging with:
    ///
    /// ```json,ignore
    /// { "foo": { "bar": false, "baz": { "quux": 42 } } }
    /// ```
    ///
    /// becomes this:
    ///
    /// ```json,ignore
    /// { "foo": { "bar": false, "baz": { "qux": true, "quux": 42 } } }
    /// ```
    Deep,

    /// Collection types are replaced entirely.
    ///
    /// That is, given:
    ///
    /// ```json,ignore
    /// { "foo": { "bar": true, "baz": { "qux": true } }, "quux": true }
    /// ```
    ///
    /// merging with:
    ///
    /// ```json,ignore
    /// { "foo": { "bar": false } }
    /// ```
    ///
    /// becomes this:
    ///
    /// ```json,ignore
    /// { "foo": { "bar": false }, "quux": true }
    /// ```
    Shallow,
}

impl MergeStrategy {
    pub(super) fn is_shallow(&self) -> bool {
        match self {
            Self::Shallow => true,
            _ => false,
        }
    }

    pub(super) fn is_deep(&self) -> bool {
        match self {
            Self::Deep => true,
            _ => false,
        }
    }
}

impl Default for MergeStrategy {
    fn default() -> Self {
        Self::Deep
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
        left.merge(right, false);
        assert_eq!(left, Collection::<()>::any());

        // merge two separate known fields
        let mut left: Collection<&str> = BTreeMap::from([("foo", Kind::null())]).into();
        let right: Collection<&str> = BTreeMap::from([("bar", Kind::bytes())]).into();
        left.merge(right, false);
        assert_eq!(left, {
            let mut want: Collection<&str> =
                BTreeMap::from([("foo", Kind::null()), ("bar", Kind::bytes())]).into();
            want.set_other(Kind::any());
            want
        });

        // merge similarly named known field
        let mut left: Collection<&str> = BTreeMap::from([("foo", Kind::null())]).into();
        let right: Collection<&str> = BTreeMap::from([("foo", Kind::bytes())]).into();
        left.merge(right, false);
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
        left.merge(right, false);
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
        left.merge(right, false);
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
        assert_eq!(v.other, Other::any());

        v.set_other(Kind::integer());
        assert_eq!(
            v.other,
            Other {
                any: false,
                array: false,
                object: false,
                primitives: Some(Box::new(Kind::integer()))
            }
        );
    }
}
