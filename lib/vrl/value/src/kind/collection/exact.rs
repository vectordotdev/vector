use std::{collections::BTreeMap, ops::BitOr};

use crate::Kind;

/// The exact type-state of an [`Unknown`](super::Unknown) value in a collection.
///
/// This is its own type, to avoid infinite recursion for nested collection [`Kind`]s.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Exact {
    pub(super) bytes: bool,
    pub(super) integer: bool,
    pub(super) float: bool,
    pub(super) boolean: bool,
    pub(super) timestamp: bool,
    pub(super) regex: bool,
    pub(super) null: bool,
    pub(super) object: bool,
    pub(super) array: bool,
}

impl Exact {
    pub const fn json() -> Self {
        Self {
            bytes: true,
            integer: true,
            float: true,
            boolean: true,
            timestamp: false,
            regex: false,
            null: true,
            object: true,
            array: true,
        }
    }

    /// Check if `self` is a superset of `other`.
    ///
    /// Meaning, if `other` has a type set to `true`, then `self` needs to as well.
    pub fn is_superset(&self, other: &Self) -> bool {
        if (false, true) == (self.bytes, other.bytes) {
            return false;
        }

        if (false, true) == (self.integer, other.integer) {
            return false;
        }

        if (false, true) == (self.float, other.float) {
            return false;
        }

        if (false, true) == (self.boolean, other.boolean) {
            return false;
        }

        if (false, true) == (self.timestamp, other.timestamp) {
            return false;
        }

        if (false, true) == (self.regex, other.regex) {
            return false;
        }

        if (false, true) == (self.null, other.null) {
            return false;
        }

        if (false, true) == (self.object, other.object) {
            return false;
        }

        if (false, true) == (self.array, other.array) {
            return false;
        }

        true
    }
}

impl BitOr for Exact {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self {
            bytes: self.bytes | rhs.bytes,
            integer: self.integer | rhs.integer,
            float: self.float | rhs.float,
            boolean: self.boolean | rhs.boolean,
            timestamp: self.timestamp | rhs.timestamp,
            regex: self.regex | rhs.regex,
            null: self.null | rhs.null,
            object: self.object | rhs.object,
            array: self.array | rhs.array,
        }
    }
}

impl From<Exact> for Kind {
    fn from(exact: Exact) -> Self {
        let mut kind = Self::never();

        if exact.bytes {
            kind.add_bytes();
        }

        if exact.integer {
            kind.add_integer();
        }

        if exact.float {
            kind.add_float();
        }

        if exact.boolean {
            kind.add_boolean();
        }

        if exact.timestamp {
            kind.add_timestamp();
        }

        if exact.regex {
            kind.add_regex();
        }

        if exact.null {
            kind.add_null();
        }

        if exact.object {
            kind.add_object(BTreeMap::default());
        }

        if exact.array {
            kind.add_array(BTreeMap::default());
        }

        kind
    }
}

impl From<Kind> for Exact {
    fn from(kind: Kind) -> Self {
        (&kind).into()
    }
}

impl From<&Kind> for Exact {
    fn from(kind: &Kind) -> Self {
        Self {
            bytes: kind.contains_bytes(),
            integer: kind.contains_integer(),
            float: kind.contains_float(),
            boolean: kind.contains_boolean(),
            timestamp: kind.contains_timestamp(),
            regex: kind.contains_regex(),
            null: kind.contains_null(),
            object: kind.contains_object(),
            array: kind.contains_array(),
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
            this: Exact,
            other: Exact,
            want: bool,
        }

        for (title, TestCase { this, other, want }) in HashMap::from([
            (
                "json comparison",
                TestCase {
                    this: Exact::json(),
                    other: Exact::json(),
                    want: true,
                },
            ),
            (
                "single matching type",
                TestCase {
                    this: Exact {
                        bytes: true,
                        integer: false,
                        float: false,
                        boolean: false,
                        timestamp: false,
                        regex: false,
                        null: false,
                        object: false,
                        array: false,
                    },
                    other: Exact {
                        bytes: true,
                        integer: false,
                        float: false,
                        boolean: false,
                        timestamp: false,
                        regex: false,
                        null: false,
                        object: false,
                        array: false,
                    },
                    want: true,
                },
            ),
            (
                "multiple matching types",
                TestCase {
                    this: Exact {
                        bytes: true,
                        integer: true,
                        float: false,
                        boolean: true,
                        timestamp: false,
                        regex: false,
                        null: false,
                        object: false,
                        array: false,
                    },
                    other: Exact {
                        bytes: true,
                        integer: true,
                        float: false,
                        boolean: true,
                        timestamp: false,
                        regex: false,
                        null: false,
                        object: false,
                        array: false,
                    },
                    want: true,
                },
            ),
            (
                "matching superset",
                TestCase {
                    this: Exact {
                        bytes: true,
                        integer: true,
                        float: false,
                        boolean: true,
                        timestamp: false,
                        regex: false,
                        null: false,
                        object: false,
                        array: false,
                    },
                    other: Exact {
                        bytes: true,
                        integer: true,
                        float: false,
                        boolean: false,
                        timestamp: false,
                        regex: false,
                        null: false,
                        object: false,
                        array: false,
                    },
                    want: true,
                },
            ),
            (
                "mismatched superset",
                TestCase {
                    this: Exact {
                        bytes: true,
                        integer: true,
                        float: false,
                        boolean: true,
                        timestamp: false,
                        regex: false,
                        null: false,
                        object: false,
                        array: false,
                    },
                    other: Exact {
                        bytes: true,
                        integer: true,
                        float: false,
                        boolean: false,
                        timestamp: false,
                        regex: true,
                        null: false,
                        object: false,
                        array: false,
                    },
                    want: false,
                },
            ),
        ]) {
            assert_eq!(this.is_superset(&other), want, "{title}");
        }
    }
}
