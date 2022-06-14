use std::collections::{BTreeMap, BTreeSet};

use lookup::LookupBuf;
use value::{
    kind::{merge, nest, Collection, Field, Unknown},
    Kind,
};

/// The definition of a schema.
///
/// This struct contains all the information needed to inspect the schema of an event emitted by
/// a source/transform.
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct Definition {
    /// The collection of fields and their types stored in the event.
    collection: Collection<Field>,

    /// Semantic meaning assigned to fields within the collection.
    ///
    /// The value within this map points to a path inside the `collection`. It is an invalid state
    /// for there to be a meaning pointing to a non-existing path in the collection.
    meaning: BTreeMap<String, MeaningPointer>,
}

/// In regular use, a semantic meaning points to exactly _one_ location in the collection. However,
/// when merging two [`Definition`]s, we need to be able to allow for two definitions with the same
/// semantic meaning identifier to be merged together.
///
/// We cannot error when this happens, because a follow-up component (such as the `remap`
/// transform) might rectify the issue of having a semantic meaning with multiple pointers.
///
/// Because of this, we encapsulate this state in an enum. The schema validation step done by the
/// sink builder, will return an error if the definition stores an "invalid" meaning pointer.
#[derive(Clone, Debug, PartialEq, PartialOrd)]
enum MeaningPointer {
    Valid(LookupBuf),
    Invalid(BTreeSet<LookupBuf>),
}

impl MeaningPointer {
    fn merge(self, other: Self) -> Self {
        let set = match (self, other) {
            (Self::Valid(lhs), Self::Valid(rhs)) if lhs == rhs => return Self::Valid(lhs),
            (Self::Valid(lhs), Self::Valid(rhs)) => BTreeSet::from([lhs, rhs]),
            (Self::Valid(lhs), Self::Invalid(mut rhs)) => {
                rhs.insert(lhs);
                rhs
            }
            (Self::Invalid(mut lhs), Self::Valid(rhs)) => {
                lhs.insert(rhs);
                lhs
            }
            (Self::Invalid(mut lhs), Self::Invalid(rhs)) => {
                lhs.extend(rhs);
                lhs
            }
        };

        Self::Invalid(set)
    }
}

#[cfg(test)]
impl From<&str> for MeaningPointer {
    fn from(v: &str) -> Self {
        MeaningPointer::Valid(v.into())
    }
}

#[cfg(test)]
impl From<LookupBuf> for MeaningPointer {
    fn from(v: LookupBuf) -> Self {
        MeaningPointer::Valid(v)
    }
}

impl Definition {
    /// Create an "empty" definition.
    ///
    /// This means no type information is known about the event.
    pub fn empty() -> Self {
        Self {
            collection: Collection::empty(),
            meaning: BTreeMap::default(),
        }
    }

    /// Check if the definition is "empty", meaning:
    ///
    /// 1. There are no known fields defined.
    /// 2. The unknown fields are set to "any".
    pub fn is_empty(&self) -> bool {
        self.collection.known().is_empty()
            && self.collection.unknown().map_or(false, Unknown::is_any)
    }

    /// Add type information for an event field.
    ///
    /// # Panics
    ///
    /// - Provided path is a root path (e.g. `.`).
    /// - Provided path points to a root-level array (e.g. `.[0]`).
    /// - Provided path has one or more coalesced segments (e.g. `.(foo | bar)`).
    #[must_use]
    pub fn with_field(
        mut self,
        path: impl Into<LookupBuf>,
        kind: Kind,
        meaning: Option<&str>,
    ) -> Self {
        let mut path = path.into();
        let meaning = meaning.map(ToOwned::to_owned);

        match path.get(0) {
            None => panic!("must not be a root path"),
            Some(segment) if segment.is_index() => panic!("must not start with an index"),
            _ => {}
        };

        let collection = kind
            .nest_at_path(
                &path.to_lookup(),
                nest::Strategy {
                    coalesced_path: nest::CoalescedPath::Reject,
                },
            )
            .expect("non-coalesced path used")
            .into_object()
            .expect("always object");

        self.collection.merge(
            collection,
            merge::Strategy {
                depth: merge::Depth::Deep,
                indices: merge::Indices::Keep,
            },
        );

        if let Some(meaning) = meaning {
            self.meaning.insert(meaning, MeaningPointer::Valid(path));
        }

        self
    }

    /// Add type information for an optional event field.
    ///
    /// # Panics
    ///
    /// See `Definition::require_field`.
    #[must_use]
    pub fn optional_field(
        self,
        path: impl Into<LookupBuf>,
        kind: Kind,
        meaning: Option<&str>,
    ) -> Self {
        self.with_field(path, kind.or_null(), meaning)
    }

    /// Register a semantic meaning for the definition.
    ///
    /// # Panics
    ///
    /// This method panics if the provided path points to an unknown location in the collection.
    pub fn register_known_meaning(&mut self, path: impl Into<LookupBuf>, meaning: &str) {
        let path = path.into();

        // Ensure the path exists in the collection.
        assert!(self
            .collection
            .find_known_at_path(&mut path.to_lookup())
            .ok()
            .flatten()
            .is_some());

        self.meaning
            .insert(meaning.to_owned(), MeaningPointer::Valid(path));
    }

    /// Set the kind for all unknown fields.
    #[must_use]
    pub fn unknown_fields(mut self, unknown: impl Into<Option<Kind>>) -> Self {
        self.collection.set_unknown(unknown);
        self
    }

    /// Merge `other` definition into `self`.
    ///
    /// The merge strategy for optional fields is as follows:
    ///
    /// If the field is marked as optional in both definitions, _or_ if it's optional in one,
    /// and unspecified in the other, then the field remains optional.
    ///
    /// If it's marked as "required" in either of the two definitions, then it becomes
    /// a required field in the merged definition.
    ///
    /// Note that it is allowed to have required field nested under optional fields. For
    /// example, `.foo` might be set as optional, but `.foo.bar` as required. In this case, it
    /// means that the object at `.foo` is allowed to be missing, but if it's present, then it's
    /// required to have a `bar` field.
    #[must_use]
    pub fn merge(mut self, other: Self) -> Self {
        for (other_id, other_meaning) in other.meaning {
            let meaning = match self.meaning.remove(&other_id) {
                Some(this_meaning) => this_meaning.merge(other_meaning),
                None => other_meaning,
            };

            self.meaning.insert(other_id, meaning);
        }

        self.collection.merge(
            other.collection,
            merge::Strategy {
                depth: merge::Depth::Deep,
                indices: merge::Indices::Keep,
            },
        );

        self
    }

    /// Returns a `Lookup` into an event, based on the provided `meaning`, if the meaning exists.
    pub fn meaning_path(&self, meaning: &str) -> Option<&LookupBuf> {
        match self.meaning.get(meaning) {
            Some(MeaningPointer::Valid(path)) => Some(path),
            None | Some(MeaningPointer::Invalid(_)) => None,
        }
    }

    pub fn invalid_meaning(&self, meaning: &str) -> Option<&BTreeSet<LookupBuf>> {
        match &self.meaning.get(meaning) {
            Some(MeaningPointer::Invalid(paths)) => Some(paths),
            None | Some(MeaningPointer::Valid(_)) => None,
        }
    }

    pub fn meanings(&self) -> impl Iterator<Item = (&String, &LookupBuf)> {
        self.meaning
            .iter()
            .filter_map(|(id, pointer)| match pointer {
                MeaningPointer::Valid(path) => Some((id, path)),
                MeaningPointer::Invalid(_) => None,
            })
    }

    pub fn collection(&self) -> &Collection<Field> {
        &self.collection
    }
}

impl From<Collection<Field>> for Definition {
    fn from(collection: Collection<Field>) -> Self {
        Self {
            collection,
            meaning: BTreeMap::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use super::*;

    #[test]
    fn test_required_field() {
        struct TestCase {
            path: LookupBuf,
            kind: Kind,
            meaning: Option<&'static str>,
            want: Definition,
        }

        for (
            title,
            TestCase {
                path,
                kind,
                meaning,
                want,
            },
        ) in HashMap::from([
            (
                "simple",
                TestCase {
                    path: "foo".into(),
                    kind: Kind::boolean(),
                    meaning: Some("foo_meaning"),
                    want: Definition {
                        collection: BTreeMap::from([("foo".into(), Kind::boolean())]).into(),
                        meaning: [("foo_meaning".to_owned(), "foo".into())].into(),
                    },
                },
            ),
            (
                "nested fields",
                TestCase {
                    path: LookupBuf::from_str(".foo.bar").unwrap(),
                    kind: Kind::regex().or_null(),
                    meaning: Some("foobar"),
                    want: Definition {
                        collection: BTreeMap::from([(
                            "foo".into(),
                            Kind::object(BTreeMap::from([("bar".into(), Kind::regex().or_null())])),
                        )])
                        .into(),
                        meaning: [(
                            "foobar".to_owned(),
                            LookupBuf::from_str(".foo.bar").unwrap().into(),
                        )]
                        .into(),
                    },
                },
            ),
            (
                "no meaning",
                TestCase {
                    path: "foo".into(),
                    kind: Kind::boolean(),
                    meaning: None,
                    want: Definition {
                        collection: BTreeMap::from([("foo".into(), Kind::boolean())]).into(),
                        meaning: BTreeMap::default(),
                    },
                },
            ),
        ]) {
            let mut got = Definition::empty();
            got = got.with_field(path, kind, meaning);

            assert_eq!(got, want, "{}", title);
        }
    }

    #[test]
    fn test_optional_field() {
        struct TestCase {
            path: LookupBuf,
            kind: Kind,
            meaning: Option<&'static str>,
            want: Definition,
        }

        for (
            title,
            TestCase {
                path,
                kind,
                meaning,
                want,
            },
        ) in HashMap::from([
            (
                "simple",
                TestCase {
                    path: "foo".into(),
                    kind: Kind::boolean(),
                    meaning: Some("foo_meaning"),
                    want: Definition {
                        collection: BTreeMap::from([("foo".into(), Kind::boolean().or_null())])
                            .into(),
                        meaning: [("foo_meaning".to_owned(), "foo".into())].into(),
                    },
                },
            ),
            (
                "nested fields",
                TestCase {
                    path: LookupBuf::from_str(".foo.bar").unwrap(),
                    kind: Kind::regex().or_null(),
                    meaning: Some("foobar"),
                    want: Definition {
                        collection: BTreeMap::from([(
                            "foo".into(),
                            Kind::object(BTreeMap::from([("bar".into(), Kind::regex().or_null())])),
                        )])
                        .into(),
                        meaning: [(
                            "foobar".to_owned(),
                            LookupBuf::from_str(".foo.bar").unwrap().into(),
                        )]
                        .into(),
                    },
                },
            ),
            (
                "no meaning",
                TestCase {
                    path: "foo".into(),
                    kind: Kind::boolean(),
                    meaning: None,
                    want: Definition {
                        collection: BTreeMap::from([("foo".into(), Kind::boolean().or_null())])
                            .into(),
                        meaning: BTreeMap::default(),
                    },
                },
            ),
        ]) {
            let mut got = Definition::empty();
            got = got.optional_field(path, kind, meaning);

            assert_eq!(got, want, "{}", title);
        }
    }

    #[test]
    fn test_unknown_fields() {
        let want = Definition {
            collection: Collection::from_unknown(Kind::bytes().or_integer()),
            meaning: BTreeMap::default(),
        };

        let mut got = Definition::empty();
        got = got.unknown_fields(Kind::boolean());
        got = got.unknown_fields(Kind::bytes().or_integer());

        assert_eq!(got, want);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_merge() {
        struct TestCase {
            this: Definition,
            other: Definition,
            want: Definition,
        }

        for (title, TestCase { this, other, want }) in HashMap::from([
            (
                "equal definitions",
                TestCase {
                    this: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean().or_null(),
                        )])),
                        meaning: BTreeMap::from([("foo_meaning".to_owned(), "foo".into())]),
                    },
                    other: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean().or_null(),
                        )])),
                        meaning: BTreeMap::from([("foo_meaning".to_owned(), "foo".into())]),
                    },
                    want: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean().or_null(),
                        )])),
                        meaning: BTreeMap::from([("foo_meaning".to_owned(), "foo".into())]),
                    },
                },
            ),
            (
                "this optional, other required",
                TestCase {
                    this: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean().or_null(),
                        )])),
                        meaning: BTreeMap::default(),
                    },
                    other: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        meaning: BTreeMap::default(),
                    },
                    want: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean().or_null(),
                        )])),
                        meaning: BTreeMap::default(),
                    },
                },
            ),
            (
                "this required, other optional",
                TestCase {
                    this: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        meaning: BTreeMap::default(),
                    },
                    other: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean().or_null(),
                        )])),
                        meaning: BTreeMap::default(),
                    },
                    want: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean().or_null(),
                        )])),
                        meaning: BTreeMap::default(),
                    },
                },
            ),
            (
                "this required, other required",
                TestCase {
                    this: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        meaning: BTreeMap::default(),
                    },
                    other: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        meaning: BTreeMap::default(),
                    },
                    want: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        meaning: BTreeMap::default(),
                    },
                },
            ),
            (
                "same meaning, pointing to different paths",
                TestCase {
                    this: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        meaning: BTreeMap::from([(
                            "foo".into(),
                            MeaningPointer::Valid("foo".into()),
                        )]),
                    },
                    other: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        meaning: BTreeMap::from([(
                            "foo".into(),
                            MeaningPointer::Valid("bar".into()),
                        )]),
                    },
                    want: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        meaning: BTreeMap::from([(
                            "foo".into(),
                            MeaningPointer::Invalid(BTreeSet::from(["foo".into(), "bar".into()])),
                        )]),
                    },
                },
            ),
            (
                "same meaning, pointing to same path",
                TestCase {
                    this: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        meaning: BTreeMap::from([(
                            "foo".into(),
                            MeaningPointer::Valid("foo".into()),
                        )]),
                    },
                    other: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        meaning: BTreeMap::from([(
                            "foo".into(),
                            MeaningPointer::Valid("foo".into()),
                        )]),
                    },
                    want: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        meaning: BTreeMap::from([(
                            "foo".into(),
                            MeaningPointer::Valid("foo".into()),
                        )]),
                    },
                },
            ),
        ]) {
            let got = this.merge(other);

            assert_eq!(got, want, "{}", title);
        }
    }
}
