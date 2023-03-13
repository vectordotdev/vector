use super::{Collection, Field, Index, Kind};

impl Kind {
    /// Get the inner object collection.
    ///
    /// This returns `None` if the type is not known to be an object.
    #[must_use]
    pub const fn as_object(&self) -> Option<&Collection<Field>> {
        self.object.as_ref()
    }

    /// Get a mutable reference to the inner object collection.
    ///
    /// This returns `None` if the type is not known to be an object.
    #[must_use]
    pub fn as_object_mut(&mut self) -> Option<&mut Collection<Field>> {
        self.object.as_mut()
    }

    /// Take an object `Collection` type out of the `Kind`.
    ///
    /// This returns `None` if the type is not known to be an object.
    #[must_use]
    #[allow(clippy::missing_const_for_fn /* false positive */)]
    pub fn into_object(self) -> Option<Collection<Field>> {
        self.object
    }

    /// Get the inner array collection.
    ///
    /// This returns `None` if the type is not known to be an array.
    #[must_use]
    pub const fn as_array(&self) -> Option<&Collection<Index>> {
        self.array.as_ref()
    }

    /// Get a mutable reference to the inner array collection.
    ///
    /// This returns `None` if the type is not known to be an array.
    #[must_use]
    pub fn as_array_mut(&mut self) -> Option<&mut Collection<Index>> {
        self.array.as_mut()
    }

    /// Take an array `Collection` type out of the `Kind`.
    ///
    /// This returns `None` if the type is not known to be an array.
    #[must_use]
    #[allow(clippy::missing_const_for_fn /* false positive */)]
    pub fn into_array(self) -> Option<Collection<Index>> {
        self.array
    }

    /// Returns `Kind`, with non-primitive states removed.
    ///
    /// That is, it returns `self,` but removes the `object` and `array` states.
    #[must_use]
    pub fn to_primitives(mut self) -> Self {
        self.remove_array();
        self.remove_object();
        self
    }

    /// VRL has an interesting property where accessing an undefined value "upgrades"
    /// it to a "null" value.
    /// This should be used in places those implicit upgrades can occur.
    // see: https://github.com/vectordotdev/vector/issues/13594
    #[must_use]
    pub fn upgrade_undefined(mut self) -> Self {
        if self.is_never() {
            return self;
        }
        if self.contains_undefined() {
            self = self.without_undefined().or_null();
        }
        self
    }
}

impl From<Collection<Field>> for Kind {
    fn from(collection: Collection<Field>) -> Self {
        Self::object(collection)
    }
}

impl From<Collection<Index>> for Kind {
    fn from(collection: Collection<Index>) -> Self {
        Self::array(collection)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use super::*;

    #[test]
    fn test_to_primitive() {
        struct TestCase {
            kind: Kind,
            want: Kind,
        }

        for (title, TestCase { kind, want }) in HashMap::from([
            (
                "single primitive",
                TestCase {
                    kind: Kind::bytes(),
                    want: Kind::bytes(),
                },
            ),
            (
                "multiple primitives",
                TestCase {
                    kind: Kind::integer().or_regex(),
                    want: Kind::integer().or_regex(),
                },
            ),
            (
                "array only",
                TestCase {
                    kind: Kind::array(BTreeMap::default()),
                    want: Kind::never(),
                },
            ),
            (
                "object only",
                TestCase {
                    kind: Kind::object(BTreeMap::default()),
                    want: Kind::never(),
                },
            ),
            (
                "collections removed",
                TestCase {
                    kind: Kind::timestamp()
                        .or_integer()
                        .or_object(BTreeMap::default())
                        .or_array(BTreeMap::default()),
                    want: Kind::timestamp().or_integer(),
                },
            ),
        ]) {
            assert_eq!(kind.to_primitives(), want, "{title}");
        }
    }
}
