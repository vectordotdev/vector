use super::Kind;

impl Kind {
    /// Returns `true` if all type states are valid.
    ///
    /// That is, this method only returns `true` if the object matches _all_ of the known types.
    #[must_use]
    pub fn is_any(&self) -> bool {
        self.is_bytes()
            && self.is_integer()
            && self.is_float()
            && self.is_boolean()
            && self.is_timestamp()
            && self.is_regex()
            && self.is_null()
            && self.is_array()
            && self.is_object()
    }

    /// Returns `true` if only primitive type states are valid.
    #[must_use]
    pub fn is_primitive(&self) -> bool {
        !self.is_empty() && !self.is_collection()
    }

    /// Returns `true` if only collection type states are valid.
    #[must_use]
    pub fn is_collection(&self) -> bool {
        if !self.is_object() && !self.is_array() {
            return false;
        }

        !self.is_bytes()
            && !self.is_integer()
            && !self.is_float()
            && !self.is_boolean()
            && !self.is_timestamp()
            && !self.is_regex()
            && !self.is_null()
    }

    /// Returns `true` if the type is _at least_ `bytes`.
    ///
    /// Note that other type states can also still be valid, for exact matching, also compare
    /// against `is_exact()`.
    #[must_use]
    pub fn is_bytes(&self) -> bool {
        self.bytes.is_some()
    }

    /// Returns `true` if the type is _at least_ `integer`.
    ///
    /// Note that other type states can also still be valid, for exact matching, also compare
    /// against `is_exact()`.
    #[must_use]
    pub fn is_integer(&self) -> bool {
        self.integer.is_some()
    }

    /// Returns `true` if the type is _at least_ `float`.
    ///
    /// Note that other type states can also still be valid, for exact matching, also compare
    /// against `is_exact()`.
    #[must_use]
    pub fn is_float(&self) -> bool {
        self.float.is_some()
    }

    /// Returns `true` if the type is _at least_ `boolean`.
    ///
    /// Note that other type states can also still be valid, for exact matching, also compare
    /// against `is_exact()`.
    #[must_use]
    pub fn is_boolean(&self) -> bool {
        self.boolean.is_some()
    }

    /// Returns `true` if the type is _at least_ `timestamp`.
    ///
    /// Note that other type states can also still be valid, for exact matching, also compare
    /// against `is_exact()`.
    #[must_use]
    pub fn is_timestamp(&self) -> bool {
        self.timestamp.is_some()
    }

    /// Returns `true` if the type is _at least_ `regex`.
    ///
    /// Note that other type states can also still be valid, for exact matching, also compare
    /// against `is_exact()`.
    #[must_use]
    pub fn is_regex(&self) -> bool {
        self.regex.is_some()
    }

    /// Returns `true` if the type is _at least_ `null`.
    ///
    /// Note that other type states can also still be valid, for exact matching, also compare
    /// against `is_exact()`.
    #[must_use]
    pub fn is_null(&self) -> bool {
        self.null.is_some()
    }

    /// Returns `true` if the type is _at least_ `array`.
    ///
    /// Note that other type states can also still be valid, for exact matching, also compare
    /// against `is_exact()`.
    #[must_use]
    pub fn is_array(&self) -> bool {
        self.array.is_some()
    }

    /// Returns `true` if the type is _at least_ `object`.
    ///
    /// Note that other type states can also still be valid, for exact matching, also compare
    /// against `is_exact()`.
    #[must_use]
    pub fn is_object(&self) -> bool {
        self.object.is_some()
    }

    /// Returns `true` if exactly one type is set.
    ///
    /// For example, the following:
    ///
    /// ```rust,ignore
    /// kind.is_float() && kind.is_exact()
    /// ```
    ///
    /// Returns `true` only if the type is exactly a float.
    #[must_use]
    #[allow(clippy::many_single_char_names)]
    pub fn is_exact(&self) -> bool {
        let a = self.is_bytes();
        let b = self.is_integer();
        if !(!a || !b) {
            return false;
        }

        let c = self.is_float();
        if !(!c || !a && !b) {
            return false;
        }

        let d = self.is_boolean();
        if !(!d || !a && !b && !c) {
            return false;
        }

        let e = self.is_timestamp();
        if !(!e || !a && !b && !c && !d) {
            return false;
        }

        let f = self.is_regex();
        if !(!f || !a && !b && !c && !d && !e) {
            return false;
        }

        let g = self.is_null();
        if !(!g || !a && !b && !c && !d && !e && !f) {
            return false;
        }

        let h = self.is_array();
        if !(!h || !a && !b && !c && !d && !e && !f && !g) {
            return false;
        }

        let i = self.is_object();
        if !(!i || !a && !b && !c && !d && !e && !f && !g && !h) {
            return false;
        }

        true
    }

    /// Check if `self` is a superset of `other`.
    ///
    /// Meaning, if `other` has a type defined as valid, then `self` needs to have it defined as
    /// valid as well.
    ///
    /// Collection types are recursively checked (meaning, known fields in `self` also need to be
    /// a superset of `other`.
    #[must_use]
    pub fn is_superset(&self, other: &Self) -> bool {
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

        match (self.array.as_ref(), other.array.as_ref()) {
            (None, Some(_)) => return false,
            (Some(lhs), Some(rhs)) if !lhs.is_superset(rhs) => return false,
            _ => {}
        };

        match (self.object.as_ref(), other.object.as_ref()) {
            (None, Some(_)) => return false,
            (Some(lhs), Some(rhs)) if !lhs.is_superset(rhs) => return false,
            _ => {}
        };

        true
    }

    /// Check if `self` intersects `other`.
    ///
    /// Returns `true` if there are type states common to both `self` and `other`.
    #[must_use]
    pub fn intersects(&self, other: &Self) -> bool {
        if self.is_bytes() && other.is_bytes() {
            return true;
        }

        if self.is_integer() && other.is_integer() {
            return true;
        }

        if self.is_float() && other.is_float() {
            return true;
        }

        if self.is_boolean() && other.is_boolean() {
            return true;
        }

        if self.is_timestamp() && other.is_timestamp() {
            return true;
        }

        if self.is_regex() && other.is_regex() {
            return true;
        }

        if self.is_null() && other.is_null() {
            return true;
        }

        if self.is_array() && other.is_array() {
            return true;
        }

        if self.is_object() && other.is_object() {
            return true;
        }

        false
    }

    /// Check for the "empty" state of a type.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.is_bytes()
            && !self.is_integer()
            && !self.is_float()
            && !self.is_boolean()
            && !self.is_timestamp()
            && !self.is_regex()
            && !self.is_null()
            && !self.is_array()
            && !self.is_object()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use super::*;

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
                    this: Kind::any().or_object(BTreeMap::from([("foo".into(), Kind::any())])),
                    other: Kind::any(),
                    want: true,
                },
            ),
            (
                "nested object match",
                TestCase {
                    this: Kind::empty().or_object(BTreeMap::from([(
                        "foo".into(),
                        Kind::empty().or_object(BTreeMap::from([("bar".into(), Kind::any())])),
                    )])),
                    other: Kind::empty().or_object(BTreeMap::from([(
                        "foo".into(),
                        Kind::empty().or_object(BTreeMap::from([("bar".into(), Kind::bytes())])),
                    )])),
                    want: true,
                },
            ),
            (
                "nested object mismatch",
                TestCase {
                    this: Kind::empty().or_object(BTreeMap::from([(
                        "foo".into(),
                        Kind::empty().or_object(BTreeMap::from([("bar".into(), Kind::bytes())])),
                    )])),
                    other: Kind::empty().or_object(BTreeMap::from([(
                        "foo".into(),
                        Kind::empty().or_object(BTreeMap::from([("bar".into(), Kind::integer())])),
                    )])),
                    want: false,
                },
            ),
            (
                "nested array match",
                TestCase {
                    this: Kind::empty().or_array(BTreeMap::from([(
                        0.into(),
                        Kind::empty().or_array(BTreeMap::from([(1.into(), Kind::any())])),
                    )])),
                    other: Kind::empty().or_array(BTreeMap::from([(
                        0.into(),
                        Kind::empty().or_array(BTreeMap::from([(1.into(), Kind::bytes())])),
                    )])),
                    want: true,
                },
            ),
            (
                "nested array mismatch",
                TestCase {
                    this: Kind::empty().or_array(BTreeMap::from([(
                        0.into(),
                        Kind::empty().or_array(BTreeMap::from([(1.into(), Kind::bytes())])),
                    )])),
                    other: Kind::empty().or_array(BTreeMap::from([(
                        0.into(),
                        Kind::empty().or_array(BTreeMap::from([(1.into(), Kind::integer())])),
                    )])),
                    want: false,
                },
            ),
        ]) {
            assert_eq!(this.is_superset(&other), want, "{}", title);
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
            assert_eq!(kind.is_exact(), want, "{}", title);
        }
    }
}
