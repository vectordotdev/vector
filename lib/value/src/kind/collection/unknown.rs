use lookup::OwnedValuePath;
use std::collections::BTreeMap;

use super::Collection;
use crate::Kind;

/// The type-state of "unknown" values in a collection.
///
/// That is, given a collection, it can have a set of "known" value types (e.g. we know the object
/// collection has a field `.foo` with a type `integer`), but also a singular "unknown" value type
/// (e.g. the array collection has an integer value at index 0, and is 3 values in size. We don't
/// know the exact values for indices 1 and 2, but we do know that it has to be the type defined by
/// `Unknown`).
///
/// "unknown" values can either be "undefined" or the "unknown" type.
/// For example, an array with an infinite unknown of "integer" doesn't imply that _every_
/// index contains an array. Rather, it says every index contains an "integer" or is "undefined".
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd)]
pub struct Unknown(pub(super) Inner);

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd)]
pub(super) enum Inner {
    Exact(Box<Kind>),

    /// The `Infinite` unknown kind stores non-recursive types, with the invariant that the same
    /// states set on this type also apply to its nested collection types.
    ///
    /// That is, if we have an infinite type with the `bytes` and `array` state set, then the
    /// assumption is that the array of this type also has the bytes and array state, and its array
    /// has the bytes and array state, ad infinitum.
    Infinite(Infinite),
}

impl Unknown {
    /// Returns a standard representation of an "unknown" type.
    #[must_use]
    pub(crate) fn canonicalize(&self) -> Self {
        self.to_kind().or_undefined().into()
    }

    /// Get the `any` state for `Unknown`.
    #[must_use]
    pub(crate) fn any() -> Self {
        Self::infinite(Infinite::any())
    }

    /// Get the `exact` state for `Unknown`.
    #[must_use]
    pub(crate) fn exact(kind: impl Into<Kind>) -> Self {
        Self(Inner::Exact(Box::new(kind.into())))
    }

    /// Get the `exact` state for `Unknown`.
    #[must_use]
    pub(super) fn infinite(infinite: impl Into<Infinite>) -> Self {
        Self(Inner::Infinite(infinite.into()))
    }

    /// Get the `json` state for `Unknown`.
    ///
    /// See [`Unknown::exact`] for details on the [`Option`] return value.
    #[must_use]
    pub(crate) fn json() -> Self {
        Self::infinite(Infinite::json())
    }

    /// Check if the state of `Unknown` is "any".
    #[must_use]
    pub const fn is_any(&self) -> bool {
        matches!(self.0, Inner::Infinite(infinite) if infinite.is_any())
    }

    /// Check if the state of `Unknown` is "any".
    #[must_use]
    pub const fn is_json(&self) -> bool {
        matches!(self.0, Inner::Infinite(infinite) if infinite.is_json())
    }

    /// Check if the state of `Unknown` is "exact".
    #[must_use]
    pub const fn is_exact(&self) -> bool {
        matches!(self.0, Inner::Exact(_))
    }

    /// Get the `Kind` stored in this `Unknown`.
    /// This represents the kind of any type not "known".
    /// It will always include "undefined", since unknown
    /// values are not guaranteed to exist.
    #[must_use]
    pub fn to_kind(&self) -> Kind {
        self.to_existing_kind().or_undefined()
    }

    /// Get the `Kind` stored in this `Unknown`.
    ///
    /// This represents the kind of any _EXISTING_ type not "known".
    /// This function assumes the type you are accessing actually exists.
    /// If it's an optional field, `to_kind` should be used instead.
    ///
    /// This will never have "undefined" as part of the type
    #[must_use]
    pub fn to_existing_kind(&self) -> Kind {
        let mut result = match &self.0 {
            Inner::Infinite(infinite) => (*infinite).into(),
            Inner::Exact(kind) => kind.as_ref().clone(),
        };
        result.remove_undefined();
        result
    }

    /// Check if `self` is a superset of `other`.
    ///
    /// Meaning, if `self` is `Any`, then it's always a superset of `other`, otherwise its
    /// accumulative types need to be a superset of `other`.
    pub(crate) fn is_superset(&self, other: &Self) -> Result<(), OwnedValuePath> {
        match (&self.0, &other.0) {
            (Inner::Infinite(infinite), _) if infinite.is_any() => Ok(()),
            (Inner::Infinite(infinite), Inner::Exact(rhs)) => {
                Kind::from(*infinite).is_superset(rhs)
            }
            (Inner::Exact(lhs), Inner::Exact(rhs)) => lhs
                .clone()
                .without_undefined()
                .is_superset(&rhs.clone().without_undefined()),
            (Inner::Exact(lhs), Inner::Infinite(..)) => {
                if lhs.is_any() {
                    Ok(())
                } else {
                    Err(OwnedValuePath::root())
                }
            }
            (Inner::Infinite(lhs), Inner::Infinite(rhs)) => {
                if lhs.is_superset(rhs) {
                    Ok(())
                } else {
                    Err(OwnedValuePath::root())
                }
            }
        }
    }

    /// Merge `other` into `self`, using the provided `Strategy`.
    ///
    /// If any of the two `Unknown`s is marked as "infinite", it will overwrite the finite variant.
    pub(crate) fn merge(&mut self, other: Self, overwrite: bool) {
        match (&mut self.0, other.0) {
            (Inner::Exact(lhs), Inner::Exact(rhs)) => lhs.merge_keep(*rhs, overwrite),
            (Inner::Infinite(lhs), Inner::Infinite(rhs)) => lhs.merge(rhs),
            (_, rhs @ Inner::Infinite(_)) => self.0 = rhs,
            (Inner::Infinite(_), _) => {}
        }
    }
}

impl From<Kind> for Unknown {
    fn from(kind: Kind) -> Self {
        (&kind).into()
    }
}

impl From<&Kind> for Unknown {
    fn from(kind: &Kind) -> Self {
        if kind.is_any() {
            return Self::any();
        }

        if kind.is_json() {
            return Self::json();
        }

        Self::exact(kind.clone())
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd)]
pub(super) struct Infinite {
    bytes: Option<()>,
    integer: Option<()>,
    float: Option<()>,
    boolean: Option<()>,
    timestamp: Option<()>,
    regex: Option<()>,
    null: Option<()>,
    array: Option<()>,
    object: Option<()>,
}

impl Infinite {
    const fn any() -> Self {
        Self {
            bytes: Some(()),
            integer: Some(()),
            float: Some(()),
            boolean: Some(()),
            timestamp: Some(()),
            regex: Some(()),
            null: Some(()),
            array: Some(()),
            object: Some(()),
        }
    }

    const fn json() -> Self {
        Self {
            bytes: Some(()),
            integer: Some(()),
            float: Some(()),
            boolean: Some(()),
            timestamp: None,
            regex: None,
            null: Some(()),
            array: Some(()),
            object: Some(()),
        }
    }

    #[must_use]
    pub const fn is_any(&self) -> bool {
        self.bytes.is_some()
            && self.integer.is_some()
            && self.float.is_some()
            && self.boolean.is_some()
            && self.timestamp.is_some()
            && self.regex.is_some()
            && self.null.is_some()
            && self.array.is_some()
            && self.object.is_some()
    }

    /// Returns `true` if the JSON type states are valid.
    #[must_use]
    pub const fn is_json(&self) -> bool {
        self.bytes.is_some()
            && self.integer.is_some()
            && self.float.is_some()
            && self.boolean.is_some()
            && self.timestamp.is_none()
            && self.regex.is_none()
            && self.null.is_some()
            && self.array.is_some()
            && self.object.is_some()
    }

    /// Check if `self` is a superset of `other`.
    ///
    /// Meaning, if `self` is `Any`, then it's always a superset of `other`, otherwise its
    /// accumulative types need to be a superset of `other`.
    pub(super) const fn is_superset(&self, other: &Self) -> bool {
        if let (None, Some(_)) = (self.bytes, other.bytes) {
            return false;
        };

        if let (None, Some(_)) = (self.integer, other.integer) {
            return false;
        };

        if let (None, Some(_)) = (self.float, other.float) {
            return false;
        };

        if let (None, Some(_)) = (self.boolean, other.boolean) {
            return false;
        };

        if let (None, Some(_)) = (self.timestamp, other.timestamp) {
            return false;
        };

        if let (None, Some(_)) = (self.regex, other.regex) {
            return false;
        };

        if let (None, Some(_)) = (self.null, other.null) {
            return false;
        };

        if let (None, Some(_)) = (self.array, other.array) {
            return false;
        };

        if let (None, Some(_)) = (self.object, other.object) {
            return false;
        };

        true
    }

    /// Merge `other` into `self`.
    pub(super) fn merge(&mut self, other: Self) {
        self.bytes = self.bytes.or(other.bytes);
        self.integer = self.integer.or(other.integer);
        self.float = self.float.or(other.float);
        self.boolean = self.boolean.or(other.boolean);
        self.timestamp = self.timestamp.or(other.timestamp);
        self.regex = self.regex.or(other.regex);
        self.null = self.null.or(other.null);
        self.array = self.array.or(other.array);
        self.object = self.object.or(other.object);
    }
}

impl From<Infinite> for Kind {
    fn from(infinite: Infinite) -> Self {
        let mut kind = Self::never();

        if infinite.bytes.is_some() {
            kind.add_bytes();
        }

        if infinite.integer.is_some() {
            kind.add_integer();
        }

        if infinite.float.is_some() {
            kind.add_float();
        }

        if infinite.boolean.is_some() {
            kind.add_boolean();
        }

        if infinite.timestamp.is_some() {
            kind.add_timestamp();
        }

        if infinite.regex.is_some() {
            kind.add_regex();
        }

        if infinite.null.is_some() {
            kind.add_null();
        }

        if infinite.array.is_some() {
            kind.add_array(Collection::from(infinite));
        }

        if infinite.object.is_some() {
            kind.add_object(Collection::from(infinite));
        }

        kind
    }
}

impl<T: Ord> From<Infinite> for Collection<T> {
    fn from(infinite: Infinite) -> Self {
        Self {
            known: BTreeMap::default(),
            unknown: Unknown::infinite(infinite),
        }
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
            this: Unknown,
            other: Unknown,
            want: bool,
        }

        for (title, TestCase { this, other, want }) in HashMap::from([
            (
                "any comparison",
                TestCase {
                    this: Unknown::any(),
                    other: Unknown::any(),
                    want: true,
                },
            ),
            (
                "exact/any mismatch",
                TestCase {
                    this: Unknown::json(),
                    other: Unknown::any(),
                    want: false,
                },
            ),
            (
                "any/exact match",
                TestCase {
                    this: Unknown::any(),
                    other: Unknown::json(),
                    want: true,
                },
            ),
            (
                "exact matching comparison",
                TestCase {
                    this: Unknown::json(),
                    other: Unknown::json(),
                    want: true,
                },
            ),
            (
                "exact mismatch comparison",
                TestCase {
                    this: Unknown::exact(Kind::bytes()),
                    other: Unknown::exact(Kind::integer()),
                    want: false,
                },
            ),
        ]) {
            assert_eq!(this.is_superset(&other).is_ok(), want, "{title}");
        }
    }
}
