mod exact;
mod field;
mod index;
mod unknown;

use std::collections::BTreeMap;

pub use field::Field;
pub use index::Index;
use lookup::lookup_v2::OwnedSegment;
use lookup::OwnedValuePath;
pub use unknown::Unknown;

use super::Kind;

pub trait CollectionKey {
    fn to_segment(&self) -> OwnedSegment;
}

/// The kinds of a collection (e.g. array or object).
///
/// A collection contains one or more kinds for known positions within the collection (e.g. indices
/// or fields), and contains a global "unknown" state that applies to all unknown paths.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd)]
pub struct Collection<T: Ord> {
    known: BTreeMap<T, Kind>,

    /// The kind of other unknown fields.
    ///
    /// For example, an array collection might be known to have an "integer" state at the 0th
    /// index, but it has an unknown length. It is however known that whatever length the array
    /// has, its values can only be integers or floats, so the `unknown` state is set to those two.
    unknown: Unknown,
}

impl<T: Ord + Clone> Collection<T> {
    /// Create a new collection from its parts.
    #[must_use]
    pub fn from_parts(known: BTreeMap<T, Kind>, unknown: impl Into<Kind>) -> Self {
        Self {
            known,
            unknown: unknown.into().into(),
        }
    }

    pub(super) fn canonicalize(&self) -> Self {
        let mut output = (*self).clone();

        output.unknown = self.unknown.canonicalize();

        let unknown_kind = self.unknown_kind();
        output
            .known_mut()
            .retain(|i, i_kind| *i_kind != unknown_kind);
        output
    }

    /// Create a new collection with a defined "unknown fields" value, and no known fields.
    #[must_use]
    pub fn from_unknown(unknown: impl Into<Kind>) -> Self {
        Self {
            known: BTreeMap::default(),
            unknown: unknown.into().into(),
        }
    }

    /// Create a collection kind of which there are no known and no unknown kinds.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            known: BTreeMap::default(),
            unknown: Kind::undefined().into(),
        }
    }

    /// Create a collection kind of which the encapsulated values can be any kind.
    #[must_use]
    pub fn any() -> Self {
        Self {
            known: BTreeMap::default(),
            unknown: Unknown::any(),
        }
    }

    /// Create a collection kind of which the encapsulated values can be any JSON-compatible kind.
    #[must_use]
    pub fn json() -> Self {
        Self {
            known: BTreeMap::default(),
            unknown: Unknown::json(),
        }
    }

    /// Check if the collection fields can be of any kind.
    ///
    /// This returns `false` if at least _one_ field kind is known.
    #[must_use]
    pub fn is_any(&self) -> bool {
        self.known.values().all(Kind::is_any) && self.unknown_kind().is_any()
    }

    /// Get a reference to the "known" elements in the collection.
    #[must_use]
    pub fn known(&self) -> &BTreeMap<T, Kind> {
        &self.known
    }

    /// Get a mutable reference to the "known" elements in the collection.
    #[must_use]
    pub fn known_mut(&mut self) -> &mut BTreeMap<T, Kind> {
        &mut self.known
    }

    /// Gets the type of "unknown" elements in the collection.
    /// The returned type will always have "undefined" included.
    #[must_use]
    pub fn unknown_kind(&self) -> Kind {
        self.unknown.to_kind()
    }

    /// Returns true if the unknown variant is "Exact" (vs "Infinite").
    /// This can be used to determine when to stop recursing into an unknown kind.
    /// Once the unknown is infinite, this will return false and all unknowns after that
    /// will return the same kind.
    #[must_use]
    pub fn is_unknown_exact(&self) -> bool {
        self.unknown.is_exact()
    }

    /// Returns an enum describing if the collection is empty.
    #[must_use]
    pub fn is_empty(&self) -> EmptyState {
        if self.known.is_empty() {
            if self.unknown_kind().contains_any_defined() {
                EmptyState::Maybe
            } else {
                EmptyState::Always
            }
        } else {
            EmptyState::Never
        }
    }

    /// Set all "unknown" collection elements to the given kind.
    pub fn set_unknown(&mut self, unknown: impl Into<Kind>) {
        self.unknown = unknown.into().into();
    }

    /// Returns a new collection with the unknown set.
    #[must_use]
    pub fn with_unknown(mut self, unknown: impl Into<Kind>) -> Self {
        self.set_unknown(unknown);
        self
    }

    /// Returns a new collection that includes the known key.
    #[must_use]
    pub fn with_known(mut self, key: impl Into<T>, kind: Kind) -> Self {
        self.known_mut().insert(key.into(), kind);
        self
    }

    /// Given a collection of known and unknown types, merge the known types with the unknown type,
    /// and remove a reference to the known types.
    ///
    /// That is, given an object with field "foo" as integer, "bar" as bytes and unknown fields as
    /// timestamp, after calling this function, the object has no known fields, and all unknown
    /// fields are marked as either an integer, bytes or timestamp.
    ///
    /// Recursively known fields are left untouched. For example, an object with a field "foo" that
    /// has an object with a field "bar" results in a collection of which any field can have an
    /// object that has a field "bar".
    pub fn anonymize(&mut self) {
        let known_unknown = self
            .known
            .values_mut()
            .reduce(|lhs, rhs| {
                lhs.merge_keep(rhs.clone(), false);
                lhs
            })
            .cloned()
            .unwrap_or(Kind::never());

        self.known.clear();
        self.unknown = self.unknown.to_kind().union(known_unknown).into();
    }

    /// Merge the `other` collection into `self`.
    ///
    /// The following merge strategies are applied.
    ///
    /// For *known fields*:
    ///
    /// - If a field exists in both collections, their `Kind`s are merged, or the `other` fields
    ///   are used (depending on the configured [`Strategy`](merge::Strategy)).
    ///
    /// - If a field exists in one but not the other, the field is merged with the "unknown"
    ///   of the other if it exists, or just the field is used otherwise.
    ///
    /// For *unknown fields or indices*:
    ///
    /// - Both `Unknown`s are merged, similar to merging two `Kind`s.
    pub fn merge(&mut self, mut other: Self, overwrite: bool) {
        for (key, self_kind) in &mut self.known {
            if let Some(other_kind) = other.known.remove(key) {
                if overwrite {
                    *self_kind = other_kind;
                } else {
                    self_kind.merge_keep(other_kind, overwrite);
                }
            } else if other.unknown_kind().contains_any_defined() {
                if overwrite {
                    // the specific field being merged isn't guaranteed to exist, so merge it with the known type of self
                    *self_kind = other
                        .unknown_kind()
                        .without_undefined()
                        .union(self_kind.clone());
                } else {
                    self_kind.merge_keep(other.unknown_kind(), overwrite);
                }
            } else if !overwrite {
                // other is missing this field, which returns null
                self_kind.add_undefined();
            }
        }

        let self_unknown_kind = self.unknown_kind();
        if self_unknown_kind.contains_any_defined() {
            for (key, mut other_kind) in other.known {
                if !overwrite {
                    other_kind.merge_keep(self_unknown_kind.clone(), overwrite);
                }
                self.known_mut().insert(key, other_kind);
            }
        } else if overwrite {
            self.known.extend(other.known);
        } else {
            for (key, other_kind) in other.known {
                // self is missing this field, which returns null
                self.known.insert(key, other_kind.or_undefined());
            }
        }
        self.unknown.merge(other.unknown, overwrite);
    }

    /// Return the reduced `Kind` of the items within the collection.
    /// This only returns the type of _defined_ values in the collection. Accessing
    /// a non-existing value can return `undefined` which is not added to the type here.
    #[must_use]
    pub fn reduced_kind(&self) -> Kind {
        self.known
            .values()
            .cloned()
            .reduce(|lhs, rhs| lhs.union(rhs))
            .unwrap_or_else(Kind::never)
            .union(self.unknown_kind().without_undefined())
    }
}

impl<T: Ord + Clone + CollectionKey> Collection<T> {
    /// Check if `self` is a superset of `other`.
    ///
    /// Meaning, for all known fields in `other`, if the field also exists in `self`, then its type
    /// needs to be a subset of `self`, otherwise its type needs to be a subset of self's
    /// `unknown`.
    ///
    /// If `self` has known fields not defined in `other`, then `other`'s `unknown` must be
    /// a superset of those fields defined in `self`.
    ///
    /// Additionally, other's `unknown` type needs to be a subset of `self`'s.
    ///
    /// # Errors
    /// If the type is not a superset, a path to one field that doesn't match is returned.
    /// This is mostly useful for debugging.
    pub fn is_superset(&self, other: &Self) -> Result<(), OwnedValuePath> {
        // `self`'s `unknown` needs to be  a superset of `other`'s.
        self.unknown
            .is_superset(&other.unknown)
            .map_err(|path| path.with_field_prefix("<unknown>"))?;

        // All known fields in `other` need to either be a subset of a matching known field in
        // `self`, or a subset of self's `unknown` type state.
        for (key, other_kind) in &other.known {
            match self.known.get(key) {
                Some(self_kind) => {
                    self_kind
                        .is_superset(other_kind)
                        .map_err(|path| path.with_segment_prefix(key.to_segment()))?;
                }
                None => {
                    self.unknown_kind()
                        .is_superset(other_kind)
                        .map_err(|path| path.with_segment_prefix(key.to_segment()))?;
                }
            }
        }

        // All known fields in `self` not known in `other` need to be a superset of other's
        // `unknown` type state.
        for (key, self_kind) in &self.known {
            if other.known.get(key).is_none() {
                self_kind
                    .is_superset(&other.unknown_kind())
                    .map_err(|path| path.with_segment_prefix(key.to_segment()))?;
            }
        }

        Ok(())
    }
}

pub trait CollectionRemove {
    type Key: Ord;

    fn remove_known(&mut self, key: &Self::Key);
}

/// Collections have an "unknown" component, so it can't know in all cases if the value this
/// collection represents is actually empty/not empty, so the state is represented with 3 variants.
#[derive(Debug)]
pub enum EmptyState {
    // The collection is guaranteed to be empty.
    Always,
    // The collection may or may not actually be empty. There is not enough type information to
    // determine. (There are unknown fields/indices that may exist, but there are no known values.)
    Maybe,
    // The collection is guaranteed to NOT be empty.
    Never,
}

impl<T: Ord> From<BTreeMap<T, Kind>> for Collection<T> {
    fn from(known: BTreeMap<T, Kind>) -> Self {
        Self {
            known,
            unknown: Kind::undefined().into(),
        }
    }
}

impl std::fmt::Display for Collection<Field> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.unknown_kind().contains_any_defined() || self.known.is_empty() {
            // Simple representation, we can improve upon this in the future.
            return f.write_str("object");
        }

        f.write_str("{ ")?;

        let mut known = self.known.iter().peekable();
        while let Some((key, kind)) = known.next() {
            write!(f, "{key}: {kind}")?;
            if known.peek().is_some() {
                f.write_str(", ")?;
            }
        }

        f.write_str(" }")?;

        Ok(())
    }
}

impl std::fmt::Display for Collection<Index> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.unknown_kind().contains_any_defined() || self.known.is_empty() {
            // Simple representation, we can improve upon this in the future.
            return f.write_str("array");
        }

        f.write_str("[")?;

        let mut known = self.known.iter().peekable();

        // This expects the invariant to hold that an array without "unknown"
        // fields cannot have known fields with non-incremental indices. That
        // is, an array of 5 elements has to define index 0 to 4, otherwise
        // "unknown" has to be defined.
        while let Some((_, kind)) = known.next() {
            kind.fmt(f)?;
            if known.peek().is_some() {
                f.write_str(", ")?;
            }
        }

        f.write_str("]")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    impl CollectionKey for &'static str {
        fn to_segment(&self) -> OwnedSegment {
            OwnedSegment::Field((*self).to_string())
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_is_superset() {
        struct TestCase {
            this: Collection<&'static str>,
            other: Collection<&'static str>,
            want: bool,
        }

        for (title, TestCase { this, other, want }) in HashMap::from([
            (
                "any comparison",
                TestCase {
                    this: Collection::any(),
                    other: Collection::any(),
                    want: true,
                },
            ),
            (
                "exact/any mismatch",
                TestCase {
                    this: Collection::json(),
                    other: Collection::any(),
                    want: false,
                },
            ),
            (
                "unknown match",
                TestCase {
                    this: Collection::from_unknown(Kind::regex().or_null()),
                    other: Collection::from_unknown(Kind::regex()),
                    want: true,
                },
            ),
            (
                "unknown mis-match",
                TestCase {
                    this: Collection::from_unknown(Kind::regex().or_null()),
                    other: Collection::from_unknown(Kind::bytes()),
                    want: false,
                },
            ),
            (
                "other-known match",
                TestCase {
                    this: Collection::from_parts(
                        BTreeMap::from([("bar", Kind::bytes())]),
                        Kind::regex().or_null(),
                    ),
                    other: Collection::from_parts(
                        BTreeMap::from([("foo", Kind::regex()), ("bar", Kind::bytes())]),
                        Kind::regex(),
                    ),
                    want: true,
                },
            ),
            (
                "other-known mis-match",
                TestCase {
                    this: Collection::from_parts(
                        BTreeMap::from([("foo", Kind::integer()), ("bar", Kind::bytes())]),
                        Kind::regex().or_null(),
                    ),
                    other: Collection::from_parts(
                        BTreeMap::from([("foo", Kind::regex()), ("bar", Kind::bytes())]),
                        Kind::regex(),
                    ),
                    want: false,
                },
            ),
            (
                "self-known match",
                TestCase {
                    this: Collection::from_parts(
                        BTreeMap::from([
                            ("foo", Kind::bytes().or_integer()),
                            ("bar", Kind::bytes().or_integer()),
                        ]),
                        Kind::bytes().or_integer(),
                    ),
                    other: Collection::from_unknown(Kind::bytes().or_integer()),
                    want: false,
                },
            ),
            (
                "self-known mis-match",
                TestCase {
                    this: Collection::from_parts(
                        BTreeMap::from([("foo", Kind::integer()), ("bar", Kind::bytes())]),
                        Kind::bytes().or_integer(),
                    ),
                    other: Collection::from_unknown(Kind::bytes().or_integer()),
                    want: false,
                },
            ),
            (
                "unknown superset of known",
                TestCase {
                    this: Collection::from_parts(BTreeMap::new(), Kind::bytes().or_integer()),
                    other: Collection::empty()
                        .with_known("foo", Kind::integer())
                        .with_known("bar", Kind::bytes()),
                    want: true,
                },
            ),
            (
                "unknown not superset of known",
                TestCase {
                    this: Collection::from_parts(BTreeMap::new(), Kind::bytes().or_integer()),
                    other: Collection::empty().with_known("foo", Kind::float()),
                    want: false,
                },
            ),
        ]) {
            assert_eq!(this.is_superset(&other).is_ok(), want, "{title}");
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_merge() {
        struct TestCase {
            this: Collection<&'static str>,
            other: Collection<&'static str>,
            overwrite: bool,
            want: Collection<&'static str>,
        }

        for (
            title,
            TestCase {
                mut this,
                other,
                overwrite: strategy,
                want,
            },
        ) in [
            (
                "any merge (deep)",
                TestCase {
                    this: Collection::any(),
                    other: Collection::any(),
                    overwrite: false,
                    want: Collection::any(),
                },
            ),
            (
                "any merge (shallow)",
                TestCase {
                    this: Collection::any(),
                    other: Collection::any(),
                    overwrite: true,
                    want: Collection::any(),
                },
            ),
            (
                "json merge (deep)",
                TestCase {
                    this: Collection::json(),
                    other: Collection::json(),
                    overwrite: false,
                    want: Collection::json(),
                },
            ),
            (
                "json merge (shallow)",
                TestCase {
                    this: Collection::json(),
                    other: Collection::json(),
                    overwrite: true,
                    want: Collection::json(),
                },
            ),
            (
                "any w/ json merge (deep)",
                TestCase {
                    this: Collection::any(),
                    other: Collection::json(),
                    overwrite: false,
                    want: Collection::any(),
                },
            ),
            (
                "any w/ json merge (shallow)",
                TestCase {
                    this: Collection::any(),
                    other: Collection::json(),
                    overwrite: true,
                    want: Collection::any(),
                },
            ),
            (
                "merge same knowns (deep)",
                TestCase {
                    this: Collection::from(BTreeMap::from([("foo", Kind::integer())])),
                    other: Collection::from(BTreeMap::from([("foo", Kind::bytes())])),
                    overwrite: false,
                    want: Collection::from(BTreeMap::from([("foo", Kind::integer().or_bytes())])),
                },
            ),
            (
                "merge same knowns (shallow)",
                TestCase {
                    this: Collection::from(BTreeMap::from([("foo", Kind::integer())])),
                    other: Collection::from(BTreeMap::from([("foo", Kind::bytes())])),
                    overwrite: true,
                    want: Collection::from(BTreeMap::from([("foo", Kind::bytes())])),
                },
            ),
            (
                "append different knowns (deep)",
                TestCase {
                    this: Collection::from(BTreeMap::from([("foo", Kind::integer())])),
                    other: Collection::from(BTreeMap::from([("bar", Kind::bytes())])),
                    overwrite: false,
                    want: Collection::from(BTreeMap::from([
                        ("foo", Kind::integer().or_undefined()),
                        ("bar", Kind::bytes().or_undefined()),
                    ])),
                },
            ),
            (
                "append different knowns (shallow)",
                TestCase {
                    this: Collection::from(BTreeMap::from([("foo", Kind::integer())])),
                    other: Collection::from(BTreeMap::from([("bar", Kind::bytes())])),
                    overwrite: true,
                    want: Collection::from(BTreeMap::from([
                        ("foo", Kind::integer()),
                        ("bar", Kind::bytes()),
                    ])),
                },
            ),
            (
                "merge/append same/different knowns (deep)",
                TestCase {
                    this: Collection::from(BTreeMap::from([("foo", Kind::integer())])),
                    other: Collection::from(BTreeMap::from([
                        ("foo", Kind::bytes()),
                        ("bar", Kind::boolean()),
                    ])),
                    overwrite: false,
                    want: Collection::from(BTreeMap::from([
                        ("foo", Kind::integer().or_bytes()),
                        ("bar", Kind::boolean().or_undefined()),
                    ])),
                },
            ),
            (
                "merge/append same/different knowns (shallow)",
                TestCase {
                    this: Collection::from(BTreeMap::from([("foo", Kind::integer())])),
                    other: Collection::from(BTreeMap::from([
                        ("foo", Kind::bytes()),
                        ("bar", Kind::boolean()),
                    ])),
                    overwrite: true,
                    want: Collection::from(BTreeMap::from([
                        ("foo", Kind::bytes()),
                        ("bar", Kind::boolean()),
                    ])),
                },
            ),
            (
                "merge unknowns (deep)",
                TestCase {
                    this: Collection::from_unknown(Kind::bytes()),
                    other: Collection::from_unknown(Kind::integer()),
                    overwrite: false,
                    want: Collection::from_unknown(Kind::bytes().or_integer()),
                },
            ),
            (
                "merge unknowns (shallow)",
                TestCase {
                    this: Collection::from_unknown(Kind::bytes()),
                    other: Collection::from_unknown(Kind::integer()),
                    overwrite: true,
                    want: Collection::from_unknown(Kind::bytes().or_integer()),
                },
            ),
            (
                "merge known with specific unknown",
                TestCase {
                    this: Collection::from(BTreeMap::from([("a", Kind::integer())])),
                    other: Collection::from_unknown(Kind::float()),
                    overwrite: true,
                    want: Collection::from(BTreeMap::from([("a", Kind::integer().or_float())]))
                        .with_unknown(Kind::float().or_undefined()),
                },
            ),
        ] {
            this.merge(other, strategy);
            assert_eq!(this, want, "{title}");
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_anonymize() {
        struct TestCase {
            this: Collection<&'static str>,
            want: Collection<&'static str>,
        }

        for (title, TestCase { mut this, want }) in HashMap::from([
            (
                "no knowns / any unknown",
                TestCase {
                    this: Collection::any(),
                    want: Collection::any(),
                },
            ),
            (
                "no knowns / json unknown",
                TestCase {
                    this: Collection::json(),
                    want: Collection::json(),
                },
            ),
            (
                "integer known / no unknown",
                TestCase {
                    this: Collection::from(BTreeMap::from([("foo", Kind::integer())])),
                    want: Collection::from_unknown(Kind::integer().or_undefined()),
                },
            ),
            (
                "integer known / any unknown",
                TestCase {
                    this: {
                        let mut v = Collection::from(BTreeMap::from([("foo", Kind::integer())]));
                        v.set_unknown(Kind::any());
                        v
                    },
                    want: Collection::from_unknown(Kind::any()),
                },
            ),
            (
                "integer known / byte unknown",
                TestCase {
                    this: {
                        let mut v = Collection::from(BTreeMap::from([("foo", Kind::integer())]));
                        v.set_unknown(Kind::bytes());
                        v
                    },
                    want: Collection::from_unknown(Kind::integer().or_bytes().or_undefined()),
                },
            ),
            (
                "boolean/array known / byte/object unknown",
                TestCase {
                    this: {
                        let mut v = Collection::from(BTreeMap::from([
                            ("foo", Kind::boolean()),
                            (
                                "bar",
                                Kind::array(BTreeMap::from([(0.into(), Kind::timestamp())])),
                            ),
                        ]));
                        v.set_unknown(
                            Kind::bytes()
                                .or_object(BTreeMap::from([("baz".into(), Kind::regex())])),
                        );
                        v
                    },
                    want: Collection::from_unknown(
                        Kind::boolean()
                            .or_array(BTreeMap::from([(0.into(), Kind::timestamp())]))
                            .or_bytes()
                            .or_object(BTreeMap::from([("baz".into(), Kind::regex())]))
                            .or_undefined(),
                    ),
                },
            ),
        ]) {
            this.anonymize();

            assert_eq!(this, want, "{title}");
        }
    }

    #[test]
    fn test_display_field() {
        struct TestCase {
            this: Collection<Field>,
            want: &'static str,
        }

        for (title, TestCase { this, want }) in HashMap::from([
            (
                "any",
                TestCase {
                    this: Collection::any(),
                    want: "object",
                },
            ),
            (
                "unknown",
                TestCase {
                    this: Collection::from_unknown(Kind::null()),
                    want: "object",
                },
            ),
            (
                "known single",
                TestCase {
                    this: BTreeMap::from([("foo".into(), Kind::null())]).into(),
                    want: r#"{ foo: null }"#,
                },
            ),
            (
                "known multiple",
                TestCase {
                    this: BTreeMap::from([
                        ("1".into(), Kind::null()),
                        ("2".into(), Kind::boolean()),
                    ])
                    .into(),
                    want: r#"{ "1": null, "2": boolean }"#,
                },
            ),
            (
                "known multiple, nested",
                TestCase {
                    this: BTreeMap::from([
                        ("1".into(), Kind::null()),
                        (
                            "2".into(),
                            Kind::object(BTreeMap::from([("3".into(), Kind::integer())])),
                        ),
                    ])
                    .into(),
                    want: r#"{ "1": null, "2": { "3": integer } }"#,
                },
            ),
        ]) {
            assert_eq!(this.to_string(), want.to_string(), "{title}");
        }
    }

    #[test]
    fn test_display_index() {
        struct TestCase {
            this: Collection<Index>,
            want: &'static str,
        }

        for (title, TestCase { this, want }) in HashMap::from([
            (
                "any",
                TestCase {
                    this: Collection::any(),
                    want: "array",
                },
            ),
            (
                "unknown",
                TestCase {
                    this: Collection::from_unknown(Kind::null()),
                    want: "array",
                },
            ),
            (
                "known single",
                TestCase {
                    this: BTreeMap::from([(0.into(), Kind::null())]).into(),
                    want: r#"[null]"#,
                },
            ),
            (
                "known multiple",
                TestCase {
                    this: BTreeMap::from([(0.into(), Kind::null()), (1.into(), Kind::boolean())])
                        .into(),
                    want: r#"[null, boolean]"#,
                },
            ),
            (
                "known multiple, nested",
                TestCase {
                    this: BTreeMap::from([
                        (0.into(), Kind::null()),
                        (
                            1.into(),
                            Kind::object(BTreeMap::from([("0".into(), Kind::integer())])),
                        ),
                    ])
                    .into(),
                    want: r#"[null, { "0": integer }]"#,
                },
            ),
        ]) {
            assert_eq!(this.to_string(), want.to_string(), "{title}");
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_reduced_kind() {
        struct TestCase {
            this: Collection<&'static str>,
            want: Kind,
        }

        for (title, TestCase { this, want }) in HashMap::from([
            (
                "any",
                TestCase {
                    this: Collection::any(),
                    want: Kind::any().without_undefined(),
                },
            ),
            (
                "known bytes",
                TestCase {
                    this: BTreeMap::from([("foo", Kind::bytes())]).into(),
                    want: Kind::bytes(),
                },
            ),
            (
                "multiple known",
                TestCase {
                    this: BTreeMap::from([("foo", Kind::bytes()), ("bar", Kind::boolean())]).into(),
                    want: Kind::bytes().or_boolean(),
                },
            ),
            (
                "known bytes, unknown any",
                TestCase {
                    this: Collection::from_parts(
                        BTreeMap::from([("foo", Kind::bytes())]),
                        Kind::any(),
                    ),
                    want: Kind::any().without_undefined(),
                },
            ),
            (
                "known bytes, unknown timestamp",
                TestCase {
                    this: Collection::from_parts(
                        BTreeMap::from([("foo", Kind::bytes())]),
                        Kind::timestamp(),
                    ),
                    want: Kind::bytes().or_timestamp(),
                },
            ),
        ]) {
            assert_eq!(this.reduced_kind(), want, "{title}");
        }
    }
}
