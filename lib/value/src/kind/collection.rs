mod exact;
mod field;
mod index;
mod unknown;

use std::{borrow::Cow, collections::BTreeMap};

pub use field::Field;
pub use index::Index;
use lookup::{Lookup, Segment};
pub use unknown::Unknown;

use super::{merge, Kind};

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
    ///
    /// If this field is `None`, it means it is *known* for there to be no unknown fields. This is
    /// the case for example if you have a literal array, which either has X number of known
    /// elements, or it's an empty array with no known, but also no unknown elements.
    unknown: Option<Unknown>,
}

impl<T: Ord> Collection<T> {
    /// Create a new collection from its parts.
    #[must_use]
    pub(super) fn from_parts(known: BTreeMap<T, Kind>, unknown: impl Into<Option<Kind>>) -> Self {
        Self {
            known,
            unknown: unknown.into().map(Into::into),
        }
    }

    /// Create a new collection with a defined "unknown fields" value, and no known fields.
    #[must_use]
    pub fn from_unknown(unknown: impl Into<Option<Kind>>) -> Self {
        Self {
            known: BTreeMap::default(),
            unknown: unknown.into().map(Into::into),
        }
    }

    /// Create a collection kind of which there are no known and no unknown kinds.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            known: BTreeMap::default(),
            unknown: None,
        }
    }

    /// Create a collection kind of which the encapsulated values can be any kind.
    #[must_use]
    pub fn any() -> Self {
        Self {
            known: BTreeMap::default(),
            unknown: Some(Unknown::any()),
        }
    }

    /// Create a collection kind of which the encapsulated values can be any JSON-compatible kind.
    #[must_use]
    pub fn json() -> Self {
        Self {
            known: BTreeMap::default(),
            unknown: Some(Unknown::json()),
        }
    }

    /// Check if the collection fields can be of any kind.
    ///
    /// This returns `false` if at least _one_ field kind is known.
    #[must_use]
    pub fn is_any(&self) -> bool {
        self.known.values().all(Kind::is_any)
            && self.unknown.as_ref().map_or(false, Unknown::is_any)
    }

    /// Get the "known" and "unknown" parts of the collection.
    #[must_use]
    pub(super) fn into_parts(self) -> (BTreeMap<T, Kind>, Option<Kind>) {
        (
            self.known,
            self.unknown.map(|unknown| unknown.to_kind().into_owned()),
        )
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

    /// Get a reference to the "unknown" elements in the collection.
    ///
    /// If `None` is returned, it means all elements within the collection are known, i.e. it's
    /// a "closed" collection.
    #[must_use]
    pub fn unknown(&self) -> Option<&Unknown> {
        self.unknown.as_ref()
    }

    /// Set all "unknown" collection elements to the given kind.
    pub fn set_unknown(&mut self, unknown: impl Into<Option<Kind>>) {
        self.unknown = unknown.into().map(Into::into);
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
        let strategy = merge::Strategy {
            depth: merge::Depth::Shallow,
            indices: merge::Indices::Keep,
        };

        let known_unknown = self
            .known
            .values_mut()
            .reduce(|lhs, rhs| {
                lhs.merge(rhs.clone(), strategy);
                lhs
            })
            .cloned();

        self.known.clear();

        match (self.unknown.as_mut(), known_unknown) {
            (None, Some(rhs)) => self.unknown = Some(rhs.into()),
            (Some(lhs), Some(rhs)) => lhs.merge(rhs.into(), strategy),
            _ => {}
        };
    }

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
    #[must_use]
    pub fn is_superset(&self, other: &Self) -> bool {
        // `self`'s `unknown` needs to be  a superset of `other`'s.
        match (&self.unknown, &other.unknown) {
            (None, Some(_)) => return false,
            (Some(lhs), Some(rhs)) if !lhs.is_superset(rhs) => return false,
            _ => {}
        };

        // All known fields in `other` need to either be a subset of a matching known field in
        // `self`, or a subset of self's `unknown` type state.
        if !other
            .known
            .iter()
            .all(|(key, other_kind)| match self.known.get(key) {
                Some(self_kind) => self_kind.is_superset(other_kind),
                None => self
                    .unknown
                    .clone()
                    .map_or(false, |unknown| unknown.to_kind().is_superset(other_kind)),
            })
        {
            return false;
        }

        // All known fields in `self` not known in `other` need to be a superset of other's
        // `unknown` type state.
        self.known
            .iter()
            .all(|(key, self_kind)| match other.known.get(key) {
                Some(_) => true,
                None => other
                    .unknown
                    .as_ref()
                    .map_or(false, |unknown| self_kind.is_superset(&unknown.to_kind())),
            })
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
    pub fn merge(&mut self, mut other: Self, strategy: merge::Strategy) {
        for (key, self_kind) in &mut self.known {
            if let Some(other_kind) = other.known.remove(key) {
                if strategy.depth.is_shallow() {
                    *self_kind = other_kind;
                } else {
                    self_kind.merge(other_kind, strategy);
                }
            } else if let Some(other_unknown) = other.unknown() {
                if strategy.depth.is_shallow() {
                    *self_kind = other_unknown.to_kind().into_owned();
                } else {
                    self_kind.merge(other_unknown.to_kind().into_owned(), strategy);
                }
            }
        }

        let self_unknown_kind = self.unknown().map(|unknown| unknown.to_kind().into_owned());
        if let Some(self_unknown_kind) = self_unknown_kind {
            for (key, mut other_kind) in other.known {
                if !strategy.depth.is_shallow() {
                    other_kind.merge(self_unknown_kind.clone(), strategy);
                }
                self.known_mut().insert(key, other_kind);
            }
        } else {
            self.known.extend(other.known);
        }

        match (self.unknown.as_mut(), other.unknown) {
            (None, Some(rhs)) => self.unknown = Some(rhs),
            (Some(lhs), Some(rhs)) => lhs.merge(rhs, strategy),
            _ => {}
        };
    }

    /// Return the reduced `Kind` of the items within the collection.
    /// This only returns the type of _existing_ values in the collection. Accessing
    /// a non-existing value can return null which is not added to the type here.
    pub fn reduced_kind(&self) -> Kind {
        let strategy = merge::Strategy {
            depth: merge::Depth::Deep,
            indices: merge::Indices::Keep,
        };

        let mut kind = self
            .known
            .values()
            .cloned()
            .reduce(|mut lhs, rhs| {
                lhs.merge(rhs, strategy);
                lhs
            })
            .unwrap_or_else(Kind::never);

        if let Some(unknown) = &self.unknown {
            kind.merge(unknown.to_kind().into_owned(), strategy);
        }
        kind
    }
}

impl Collection<Field> {
    /// Find the `Kind` within the known set of fields.
    ///
    /// This currently has limited support for the first segment of the path. That is:
    ///
    /// - The path must not be root (`.`).
    /// - The path must not start with an index segment (`.[2]`)
    /// - The path must not start with a coalesced segment (`.(foo | bar)`).
    ///
    /// In all of the above cases, this method returns `Ok(None)`.
    ///
    /// # Errors
    ///
    /// See `Kind::find_at_path`.
    pub fn find_known_at_path<'a>(
        &'a self,
        path: &'a mut Lookup<'a>,
    ) -> Result<Option<Cow<'a, Kind>>, super::find::Error> {
        if let Some(Segment::Field(field)) = path.pop_front() {
            let field = Field::from(field);

            if let Some(kind) = self.known.get(&(field)) {
                return kind.find_at_path(path);
            }
        }

        Ok(None)
    }
}

impl<T: Ord> From<BTreeMap<T, Kind>> for Collection<T> {
    fn from(known: BTreeMap<T, Kind>) -> Self {
        Self {
            known,
            unknown: None,
        }
    }
}

impl std::fmt::Display for Collection<Field> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.unknown.is_some() || self.known.is_empty() {
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
        if self.unknown.is_some() || self.known.is_empty() {
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
                    want: true,
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
        ]) {
            assert_eq!(this.is_superset(&other), want, "{}", title);
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_merge() {
        struct TestCase {
            this: Collection<&'static str>,
            other: Collection<&'static str>,
            strategy: merge::Strategy,
            want: Collection<&'static str>,
        }

        for (
            title,
            TestCase {
                mut this,
                other,
                strategy,
                want,
            },
        ) in HashMap::from([
            (
                "any merge (deep)",
                TestCase {
                    this: Collection::any(),
                    other: Collection::any(),
                    strategy: merge::Strategy {
                        depth: merge::Depth::Deep,
                        indices: merge::Indices::Keep,
                    },
                    want: Collection::any(),
                },
            ),
            (
                "any merge (shallow)",
                TestCase {
                    this: Collection::any(),
                    other: Collection::any(),
                    strategy: merge::Strategy {
                        depth: merge::Depth::Shallow,
                        indices: merge::Indices::Keep,
                    },
                    want: Collection::any(),
                },
            ),
            (
                "json merge (deep)",
                TestCase {
                    this: Collection::json(),
                    other: Collection::json(),
                    strategy: merge::Strategy {
                        depth: merge::Depth::Deep,
                        indices: merge::Indices::Keep,
                    },
                    want: Collection::json(),
                },
            ),
            (
                "json merge (shallow)",
                TestCase {
                    this: Collection::json(),
                    other: Collection::json(),
                    strategy: merge::Strategy {
                        depth: merge::Depth::Shallow,
                        indices: merge::Indices::Keep,
                    },
                    want: Collection::json(),
                },
            ),
            (
                "any w/ json merge (deep)",
                TestCase {
                    this: Collection::any(),
                    other: Collection::json(),
                    strategy: merge::Strategy {
                        depth: merge::Depth::Deep,
                        indices: merge::Indices::Keep,
                    },
                    want: Collection::any(),
                },
            ),
            (
                "any w/ json merge (shallow)",
                TestCase {
                    this: Collection::any(),
                    other: Collection::json(),
                    strategy: merge::Strategy {
                        depth: merge::Depth::Shallow,
                        indices: merge::Indices::Keep,
                    },
                    want: Collection::any(),
                },
            ),
            (
                "merge same knowns (deep)",
                TestCase {
                    this: Collection::from(BTreeMap::from([("foo", Kind::integer())])),
                    other: Collection::from(BTreeMap::from([("foo", Kind::bytes())])),
                    strategy: merge::Strategy {
                        depth: merge::Depth::Deep,
                        indices: merge::Indices::Keep,
                    },
                    want: Collection::from(BTreeMap::from([("foo", Kind::integer().or_bytes())])),
                },
            ),
            (
                "merge same knowns (shallow)",
                TestCase {
                    this: Collection::from(BTreeMap::from([("foo", Kind::integer())])),
                    other: Collection::from(BTreeMap::from([("foo", Kind::bytes())])),
                    strategy: merge::Strategy {
                        depth: merge::Depth::Shallow,
                        indices: merge::Indices::Keep,
                    },
                    want: Collection::from(BTreeMap::from([("foo", Kind::bytes())])),
                },
            ),
            (
                "append different knowns (deep)",
                TestCase {
                    this: Collection::from(BTreeMap::from([("foo", Kind::integer())])),
                    other: Collection::from(BTreeMap::from([("bar", Kind::bytes())])),
                    strategy: merge::Strategy {
                        depth: merge::Depth::Deep,
                        indices: merge::Indices::Keep,
                    },
                    want: Collection::from(BTreeMap::from([
                        ("foo", Kind::integer()),
                        ("bar", Kind::bytes()),
                    ])),
                },
            ),
            (
                "append different knowns (shallow)",
                TestCase {
                    this: Collection::from(BTreeMap::from([("foo", Kind::integer())])),
                    other: Collection::from(BTreeMap::from([("bar", Kind::bytes())])),
                    strategy: merge::Strategy {
                        depth: merge::Depth::Shallow,
                        indices: merge::Indices::Keep,
                    },
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
                    strategy: merge::Strategy {
                        depth: merge::Depth::Deep,
                        indices: merge::Indices::Keep,
                    },
                    want: Collection::from(BTreeMap::from([
                        ("foo", Kind::integer().or_bytes()),
                        ("bar", Kind::boolean()),
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
                    strategy: merge::Strategy {
                        depth: merge::Depth::Shallow,
                        indices: merge::Indices::Keep,
                    },
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
                    strategy: merge::Strategy {
                        depth: merge::Depth::Deep,
                        indices: merge::Indices::Keep,
                    },
                    want: Collection::from_unknown(Kind::bytes().or_integer()),
                },
            ),
            (
                "merge unknowns (shallow)",
                TestCase {
                    this: Collection::from_unknown(Kind::bytes()),
                    other: Collection::from_unknown(Kind::integer()),
                    strategy: merge::Strategy {
                        depth: merge::Depth::Shallow,
                        indices: merge::Indices::Keep,
                    },
                    want: Collection::from_unknown(Kind::bytes().or_integer()),
                },
            ),
        ]) {
            this.merge(other, strategy);

            assert_eq!(this, want, "{}", title);
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
                    want: Collection::from_unknown(Kind::integer()),
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
                    want: Collection::from_unknown(Kind::integer().or_bytes()),
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
                            .or_object(BTreeMap::from([("baz".into(), Kind::regex())])),
                    ),
                },
            ),
        ]) {
            this.anonymize();

            assert_eq!(this, want, "{}", title);
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
            assert_eq!(this.to_string(), want.to_string(), "{}", title);
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
            assert_eq!(this.to_string(), want.to_string(), "{}", title);
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
                    want: Kind::any(),
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
                    want: Kind::any(),
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
            assert_eq!(this.reduced_kind(), want, "{}", title);
        }
    }
}
