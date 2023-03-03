use super::Kind;
use lookup::OwnedValuePath;

impl Kind {
    /// Returns `true` if all type states are valid.
    ///
    /// That is, this method only returns `true` if the object matches _all_ of the known types.
    #[must_use]
    pub const fn is_any(&self) -> bool {
        self.contains_bytes()
            && self.contains_integer()
            && self.contains_float()
            && self.contains_boolean()
            && self.contains_timestamp()
            && self.contains_regex()
            && self.contains_null()
            && self.contains_undefined()
            && self.contains_array()
            && self.contains_object()
    }

    /// Returns `true` if the JSON type states are valid.
    #[must_use]
    pub const fn is_json(&self) -> bool {
        self.contains_bytes()
            && self.contains_integer()
            && self.contains_float()
            && self.contains_boolean()
            && !self.contains_timestamp()
            && !self.contains_regex()
            && self.contains_null()
            && self.contains_undefined()
            && self.contains_array()
            && self.contains_object()
    }

    /// Returns `true` if only collection type states are valid.
    #[must_use]
    pub const fn is_collection(&self) -> bool {
        if !self.contains_object() && !self.contains_array() {
            return false;
        }

        !self.contains_bytes()
            && !self.contains_integer()
            && !self.contains_float()
            && !self.contains_boolean()
            && !self.contains_timestamp()
            && !self.contains_regex()
            && !self.contains_null()
            && !self.contains_undefined()
    }

    /// Returns `true` if the type is `bytes`.
    #[must_use]
    pub const fn is_bytes(&self) -> bool {
        self.integer.is_none()
            && self.float.is_none()
            && self.boolean.is_none()
            && self.timestamp.is_none()
            && self.regex.is_none()
            && self.null.is_none()
            && self.undefined.is_none()
            && self.array.is_none()
            && self.object.is_none()
    }

    /// Returns `true` if the type is `integer`.
    #[must_use]
    pub const fn is_integer(&self) -> bool {
        self.bytes.is_none()
            && self.float.is_none()
            && self.boolean.is_none()
            && self.timestamp.is_none()
            && self.regex.is_none()
            && self.null.is_none()
            && self.undefined.is_none()
            && self.array.is_none()
            && self.object.is_none()
    }

    /// Returns `true` if the type is `never`.
    #[must_use]
    pub const fn is_never(&self) -> bool {
        self.bytes.is_none()
            && self.integer.is_none()
            && self.float.is_none()
            && self.boolean.is_none()
            && self.timestamp.is_none()
            && self.regex.is_none()
            && self.null.is_none()
            && self.undefined.is_none()
            && self.array.is_none()
            && self.object.is_none()
    }

    /// Returns `true` if the type is `float`.
    #[must_use]
    pub const fn is_float(&self) -> bool {
        self.bytes.is_none()
            && self.integer.is_none()
            && self.boolean.is_none()
            && self.timestamp.is_none()
            && self.regex.is_none()
            && self.null.is_none()
            && self.undefined.is_none()
            && self.array.is_none()
            && self.object.is_none()
    }

    /// Returns `true` if the type is `boolean`.
    #[must_use]
    pub const fn is_boolean(&self) -> bool {
        self.bytes.is_none()
            && self.integer.is_none()
            && self.float.is_none()
            && self.timestamp.is_none()
            && self.regex.is_none()
            && self.null.is_none()
            && self.undefined.is_none()
            && self.array.is_none()
            && self.object.is_none()
    }

    /// Returns `true` if the type is `timestamp`.
    #[must_use]
    pub const fn is_timestamp(&self) -> bool {
        self.bytes.is_none()
            && self.integer.is_none()
            && self.float.is_none()
            && self.boolean.is_none()
            && self.regex.is_none()
            && self.null.is_none()
            && self.undefined.is_none()
            && self.array.is_none()
            && self.object.is_none()
    }

    /// Returns `true` if the type is `regex`.
    #[must_use]
    pub const fn is_regex(&self) -> bool {
        self.bytes.is_none()
            && self.integer.is_none()
            && self.float.is_none()
            && self.boolean.is_none()
            && self.timestamp.is_none()
            && self.null.is_none()
            && self.undefined.is_none()
            && self.array.is_none()
            && self.object.is_none()
    }

    /// Returns `true` if the type is `null`.
    #[must_use]
    pub const fn is_null(&self) -> bool {
        self.bytes.is_none()
            && self.integer.is_none()
            && self.float.is_none()
            && self.boolean.is_none()
            && self.timestamp.is_none()
            && self.regex.is_none()
            && self.undefined.is_none()
            && self.array.is_none()
            && self.object.is_none()
    }

    /// Returns `true` if the type is `undefined`.
    #[must_use]
    pub const fn is_undefined(&self) -> bool {
        self.bytes.is_none()
            && self.integer.is_none()
            && self.float.is_none()
            && self.boolean.is_none()
            && self.timestamp.is_none()
            && self.regex.is_none()
            && self.null.is_none()
            && self.array.is_none()
            && self.object.is_none()
    }

    /// Returns `true` if the type is `array`.
    #[must_use]
    pub const fn is_array(&self) -> bool {
        self.bytes.is_none()
            && self.integer.is_none()
            && self.float.is_none()
            && self.boolean.is_none()
            && self.timestamp.is_none()
            && self.regex.is_none()
            && self.null.is_none()
            && self.undefined.is_none()
            && self.object.is_none()
    }

    /// Returns `true` if the type is `object`.
    #[must_use]
    pub const fn is_object(&self) -> bool {
        self.bytes.is_none()
            && self.integer.is_none()
            && self.float.is_none()
            && self.boolean.is_none()
            && self.timestamp.is_none()
            && self.regex.is_none()
            && self.null.is_none()
            && self.undefined.is_none()
            && self.array.is_none()
    }

    /// Returns `true` if at most one type is set.
    #[must_use]
    #[allow(clippy::many_single_char_names)]
    pub const fn is_exact(&self) -> bool {
        self.is_bytes()
            || self.is_integer()
            || self.is_float()
            || self.is_boolean()
            || self.is_timestamp()
            || self.is_regex()
            || self.is_null()
            || self.is_undefined()
            || self.is_array()
            || self.is_object()
            || self.is_never()
    }

    /// Check if `self` is a superset of `other`.
    ///
    /// Meaning, if `other` has a type defined as valid, then `self` needs to have it defined as
    /// valid as well.
    ///
    /// Collection types are recursively checked (meaning, known fields in `self` also need to be
    /// a superset of `other`.
    ///
    /// # Errors
    /// If the type is not a superset, a path to one field that doesn't match is returned.
    /// This is mostly useful for debugging.
    pub fn is_superset(&self, other: &Self) -> Result<(), OwnedValuePath> {
        if let (None, Some(_)) = (self.bytes, other.bytes) {
            return Err(OwnedValuePath::root());
        };

        if let (None, Some(_)) = (self.integer, other.integer) {
            return Err(OwnedValuePath::root());
        };

        if let (None, Some(_)) = (self.float, other.float) {
            return Err(OwnedValuePath::root());
        };

        if let (None, Some(_)) = (self.boolean, other.boolean) {
            return Err(OwnedValuePath::root());
        };

        if let (None, Some(_)) = (self.timestamp, other.timestamp) {
            return Err(OwnedValuePath::root());
        };

        if let (None, Some(_)) = (self.regex, other.regex) {
            return Err(OwnedValuePath::root());
        };

        if let (None, Some(_)) = (self.null, other.null) {
            return Err(OwnedValuePath::root());
        };

        if let (None, Some(_)) = (self.undefined, other.undefined) {
            return Err(OwnedValuePath::root());
        };

        match (self.array.as_ref(), other.array.as_ref()) {
            (None, Some(_)) => return Err(OwnedValuePath::root()),
            (Some(lhs), Some(rhs)) => {
                lhs.is_superset(rhs)?;
            }
            _ => {}
        };

        match (self.object.as_ref(), other.object.as_ref()) {
            (None, Some(_)) => return Err(OwnedValuePath::root()),
            (Some(lhs), Some(rhs)) => lhs.is_superset(rhs)?,
            _ => {}
        };

        Ok(())
    }

    /// Check if `self` intersects `other`.
    ///
    /// Returns `true` if there are type states common to both `self` and `other`.
    #[must_use]
    pub const fn intersects(&self, other: &Self) -> bool {
        // a "never" type can be treated as any type
        if self.is_never() || other.is_never() {
            return true;
        }

        if self.contains_bytes() && other.contains_bytes() {
            return true;
        }

        if self.contains_integer() && other.contains_integer() {
            return true;
        }

        if self.contains_float() && other.contains_float() {
            return true;
        }

        if self.contains_boolean() && other.contains_boolean() {
            return true;
        }

        if self.contains_timestamp() && other.contains_timestamp() {
            return true;
        }

        if self.contains_regex() && other.contains_regex() {
            return true;
        }

        if self.contains_null() && other.contains_null() {
            return true;
        }

        if self.contains_undefined() && other.contains_undefined() {
            return true;
        }

        if self.contains_array() && other.contains_array() {
            return true;
        }

        if self.contains_object() && other.contains_object() {
            return true;
        }

        false
    }
}

// contains_*
impl Kind {
    /// Returns `true` if the type is _at least_ `bytes`.
    #[must_use]
    pub const fn contains_bytes(&self) -> bool {
        self.bytes.is_some() || self.is_never()
    }

    /// Returns `true` if the type is _at least_ `integer`.
    #[must_use]
    pub const fn contains_integer(&self) -> bool {
        self.integer.is_some() || self.is_never()
    }

    /// Returns `true` if the type is _at least_ `float`.
    #[must_use]
    pub const fn contains_float(&self) -> bool {
        self.float.is_some() || self.is_never()
    }

    /// Returns `true` if the type is _at least_ `boolean`.
    #[must_use]
    pub const fn contains_boolean(&self) -> bool {
        self.boolean.is_some() || self.is_never()
    }

    /// Returns `true` if the type is _at least_ `timestamp`.
    #[must_use]
    pub const fn contains_timestamp(&self) -> bool {
        self.timestamp.is_some() || self.is_never()
    }

    /// Returns `true` if the type is _at least_ `regex`.
    #[must_use]
    pub const fn contains_regex(&self) -> bool {
        self.regex.is_some() || self.is_never()
    }

    /// Returns `true` if the type is _at least_ `null`.
    #[must_use]
    pub const fn contains_null(&self) -> bool {
        self.null.is_some() || self.is_never()
    }

    /// Returns `true` if the type is _at least_ `undefined`.
    #[must_use]
    pub const fn contains_undefined(&self) -> bool {
        self.undefined.is_some() || self.is_never()
    }

    /// Returns `true` if the type can be _any_ type other than `undefined`
    #[must_use]
    pub const fn contains_any_defined(&self) -> bool {
        !self.is_undefined()
    }

    /// Returns `true` if the type is _at least_ `array`.
    #[must_use]
    pub const fn contains_array(&self) -> bool {
        self.array.is_some() || self.is_never()
    }

    /// Returns `true` if the type is _at least_ `object`.
    #[must_use]
    pub const fn contains_object(&self) -> bool {
        self.object.is_some() || self.is_never()
    }

    /// Returns `true` if the type contains _at least_ one non-collection type.
    #[must_use]
    pub const fn contains_primitive(&self) -> bool {
        self.bytes.is_some()
            || self.null.is_some()
            || self.boolean.is_some()
            || self.float.is_some()
            || self.integer.is_some()
            || self.regex.is_some()
            || self.timestamp.is_some()
            || self.undefined.is_some()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use super::*;
    use crate::kind::Collection;

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_is_superset() {
        struct TestCase {
            this: Kind,
            other: Kind,
            want: bool,
        }

        for (title, TestCase { this, other, want }) in HashMap::from([
            (
                "any comparison",
                TestCase {
                    this: Kind::any(),
                    other: Kind::any(),
                    want: true,
                },
            ),
            (
                "exact/any mismatch",
                TestCase {
                    this: Kind::json(),
                    other: Kind::any(),
                    want: false,
                },
            ),
            (
                "any-like",
                TestCase {
                    this: Kind::any().or_object(Collection::from_parts(
                        BTreeMap::from([("foo".into(), Kind::any())]),
                        Kind::any(),
                    )),
                    other: Kind::any(),
                    want: true,
                },
            ),
            (
                "no unknown vs unknown fields",
                TestCase {
                    // The object we create here has no "unknown" fields, e.g. it's a "closed"
                    // object. The `other` object _does_ have unknown field types, and thus `this`
                    // cannot be a superset of `other`.
                    this: Kind::any().or_object(BTreeMap::from([("foo".into(), Kind::any())])),
                    other: Kind::any(),
                    want: false,
                },
            ),
            (
                "nested object match",
                TestCase {
                    this: Kind::object(BTreeMap::from([(
                        "foo".into(),
                        Kind::object(BTreeMap::from([("bar".into(), Kind::any())])),
                    )])),
                    other: Kind::object(BTreeMap::from([(
                        "foo".into(),
                        Kind::object(BTreeMap::from([("bar".into(), Kind::bytes())])),
                    )])),
                    want: true,
                },
            ),
            (
                "nested object mismatch",
                TestCase {
                    this: Kind::object(BTreeMap::from([(
                        "foo".into(),
                        Kind::object(BTreeMap::from([("bar".into(), Kind::bytes())])),
                    )])),
                    other: Kind::object(BTreeMap::from([(
                        "foo".into(),
                        Kind::object(BTreeMap::from([("bar".into(), Kind::integer())])),
                    )])),
                    want: false,
                },
            ),
            (
                "nested array match",
                TestCase {
                    this: Kind::array(BTreeMap::from([(
                        0.into(),
                        Kind::array(BTreeMap::from([(1.into(), Kind::any())])),
                    )])),
                    other: Kind::array(BTreeMap::from([(
                        0.into(),
                        Kind::array(BTreeMap::from([(1.into(), Kind::bytes())])),
                    )])),
                    want: true,
                },
            ),
            (
                "nested array mismatch",
                TestCase {
                    this: Kind::array(BTreeMap::from([(
                        0.into(),
                        Kind::array(BTreeMap::from([(1.into(), Kind::bytes())])),
                    )])),
                    other: Kind::array(BTreeMap::from([(
                        0.into(),
                        Kind::array(BTreeMap::from([(1.into(), Kind::integer())])),
                    )])),
                    want: false,
                },
            ),
        ]) {
            assert_eq!(this.is_superset(&other).is_ok(), want, "{title}");
        }
    }

    #[test]
    fn test_is_exact() {
        struct TestCase {
            kind: Kind,
            want: bool,
        }

        for (title, TestCase { kind, want }) in HashMap::from([
            (
                "bytes",
                TestCase {
                    kind: Kind::bytes(),
                    want: true,
                },
            ),
            (
                "integer",
                TestCase {
                    kind: Kind::integer(),
                    want: true,
                },
            ),
            (
                "float",
                TestCase {
                    kind: Kind::float(),
                    want: true,
                },
            ),
            (
                "boolean",
                TestCase {
                    kind: Kind::boolean(),
                    want: true,
                },
            ),
            (
                "timestamp",
                TestCase {
                    kind: Kind::timestamp(),
                    want: true,
                },
            ),
            (
                "regex",
                TestCase {
                    kind: Kind::regex(),
                    want: true,
                },
            ),
            (
                "null",
                TestCase {
                    kind: Kind::null(),
                    want: true,
                },
            ),
            (
                "object",
                TestCase {
                    kind: Kind::object(BTreeMap::default()),
                    want: true,
                },
            ),
            (
                "array",
                TestCase {
                    kind: Kind::array(BTreeMap::default()),
                    want: true,
                },
            ),
            (
                "bytes & integer",
                TestCase {
                    kind: Kind::bytes().or_integer(),
                    want: false,
                },
            ),
            (
                "null & object",
                TestCase {
                    kind: Kind::null().or_object(BTreeMap::default()),
                    want: false,
                },
            ),
        ]) {
            assert_eq!(kind.is_exact(), want, "{title}");
        }
    }
}
