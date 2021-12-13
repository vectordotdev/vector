mod collection;

use std::collections::BTreeMap;

pub use collection::{Collection, Field, Index};
use lookup::{FieldBuf, Lookup, LookupBuf, Segment, SegmentBuf};

/// The type (kind) of a given value.
///
/// This struct tracks the known states a type can have. By allowing one type to have multiple
/// states, the type definition can be progressively refined.
///
/// At the start, a type is in the "any" state, meaning its type can be any of the valid states, as
/// more information becomes available, states can be removed, until one state is left.
///
/// A state without any type information (e.g. all fields are `None`) is an invalid invariant, and
/// is checked against by the API exposed by this type.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Kind {
    bytes: Option<()>,
    integer: Option<()>,
    float: Option<()>,
    boolean: Option<()>,
    timestamp: Option<()>,
    regex: Option<()>,
    null: Option<()>,
    array: Option<Collection<collection::Index>>,
    object: Option<Collection<collection::Field>>,
}

impl Kind {
    /// Get the inner object collection.
    ///
    /// This returns `None` if the type is not known to be an object.
    #[must_use]
    pub fn as_object(&self) -> Option<&Collection<collection::Field>> {
        self.object.as_ref()
    }

    /// Get the inner array collection.
    ///
    /// This returns `None` if the type is not known to be an array.
    #[must_use]
    pub fn as_array(&self) -> Option<&Collection<collection::Index>> {
        self.array.as_ref()
    }

    /// Check if other is contained within self.
    ///
    /// FIXME(Jean): doesn't yet work for nested collection kinds.
    #[must_use]
    pub fn contains(&self, other: &Self) -> bool {
        (self.is_bytes() && other.is_bytes())
            || (self.is_integer() && other.is_integer())
            || (self.is_float() && other.is_float())
            || (self.is_boolean() && other.is_boolean())
            || (self.is_timestamp() && other.is_timestamp())
            || (self.is_regex() && other.is_regex())
            || (self.is_null() && other.is_null())
            || (self.is_array() && other.is_array())
            || (self.is_object() && other.is_object())
    }

    /// Merge `other` type into `self`.
    ///
    /// Collection types are recursively merged.
    pub fn merge(&mut self, other: Self) {
        // Merge primitive types by setting their state to "present" if needed.
        self.bytes = self.bytes.or(other.bytes);
        self.integer = self.integer.or(other.integer);
        self.float = self.float.or(other.float);
        self.boolean = self.boolean.or(other.boolean);
        self.timestamp = self.timestamp.or(other.timestamp);
        self.regex = self.regex.or(other.regex);
        self.null = self.null.or(other.null);

        // Merge collection types by picking one if the other is none, or merging both..
        self.array = match (self.array.take(), other.array) {
            (None, None) => None,
            (this @ Some(..), None) => this,
            (None, other @ Some(..)) => other,
            (Some(mut this), Some(other)) => {
                this.merge(other);
                Some(this)
            }
        };
        self.object = match (self.object.take(), other.object) {
            (None, None) => None,
            (this @ Some(..), None) => this,
            (None, other @ Some(..)) => other,
            (Some(mut this), Some(other)) => {
                this.merge(other);
                Some(this)
            }
        };
    }

    /// Nest the given [`Kind`] into a provided path.
    ///
    /// For example, given an `integer` kind and a path `.foo`, a new `Kind` is returned that is
    /// known to be an object, of which the `foo` field is known to be an `integer`.
    #[must_use]
    pub fn nest_at_path(mut self, path: &Lookup<'_>) -> Self {
        for segment in path.iter().rev() {
            match segment {
                Segment::Field(lookup::Field { name, .. }) => {
                    let field = (*name).to_owned();
                    let map = BTreeMap::from([(field.into(), self)]);

                    self = Self::object(map);
                }
                Segment::Coalesce(fields) => {
                    // FIXME(Jean): this is incorrect, since we need to handle all fields. This bug
                    // already existed in the existing `vrl::TypeDef` implementation, and since
                    // we're doing alway with path coalescing, let's not bother with fixing this
                    // for now.
                    let field = fields.last().expect("at least one").name.to_owned();
                    let map = BTreeMap::from([(field.into(), self)]);

                    self = Self::object(map);
                }
                Segment::Index(index) => {
                    // For negative indices, we have to mark the array contents as unknown, since
                    // we can't be sure of the size of the array.
                    let collection = if index.is_negative() {
                        Collection::any()
                    } else {
                        #[allow(clippy::cast_sign_loss)]
                        let index = *index as usize;
                        let map = BTreeMap::from([(index.into(), self)]);
                        Collection::from(map)
                    };

                    self = Self::array(collection);
                }
            }
        }

        self
    }

    /// Find the `Kind` at the given path.
    ///
    /// if the path points to an unknown location, `None` is returned.
    #[must_use]
    pub fn find_at_path(&self, path: LookupBuf) -> Option<Self> {
        let mut iter = path.into_iter();

        let kind = match iter.next() {
            // We've reached the end of the path segments, so return whatever kind we're currently
            // into.
            None => return Some(self.clone()),

            // We have one or more segments to parse, do so and optionally recursively call this
            // function for the relevant kind.
            Some(segment) => match segment {
                SegmentBuf::Coalesce(fields) => match self.as_object() {
                    // We got a coalesced field, but we don't have an object to fetch the path
                    // from, so there's no `kind` to find at the given path.
                    None => return None,

                    // We have an object, but know nothing of its fields, so all we can say is that
                    // there _might_ be a field at the given path, but it could be of any type.
                    Some(collection) if collection.is_any() => return Some(Kind::any()),

                    // We have an object with one or more known fields. Try to find the requested
                    // field within the collection, or use the default "other" field type info.
                    Some(collection) => fields
                        .into_iter()
                        .find_map(|field| collection.known().get(&field.into()).cloned())
                        .unwrap_or_else(|| collection.other().into_owned()),
                },

                SegmentBuf::Field(FieldBuf { name: field, .. }) => match self.as_object() {
                    // We got a field, but we don't have an object to fetch the path from, so
                    // there's no `kind` to find at the given path.
                    None => return None,

                    // We have an object, but know nothing of its fields, so all we can say is that
                    // there _might_ be a field at the given path, but it could be of any type.
                    Some(collection) if collection.is_any() => return Some(Kind::any()),

                    // We have an object with one or more known fields. Try to find the requested
                    // field within the collection, or use the default "other" field type info.
                    Some(collection) => collection
                        .known()
                        .get(&field.into())
                        .cloned()
                        .unwrap_or_else(|| collection.other().into_owned()),
                },

                SegmentBuf::Index(index) => match self.as_array() {
                    // We got an index, but we don't have an array to index into, so there's no
                    // `kind` to find at the given path.
                    None => return None,

                    // If we're trying to get a negative index, we have to return "any", since we
                    // never have a full picture of the shape of an array, so we can't index from
                    // the end of the array.
                    Some(_) if index.is_negative() => return Some(Kind::any()),

                    #[allow(clippy::cast_sign_loss)]
                    Some(collection) => collection
                        .known()
                        .get(&(index as usize).into())
                        .cloned()
                        .unwrap_or_else(|| collection.other().into_owned()),
                },
            },
        };

        kind.find_at_path(LookupBuf::from_segments(iter.collect()))
    }
}

// Initializer functions.
impl Kind {
    /// The "any" type state.
    ///
    /// This state implies all states for the type are valid. There is no known information that
    /// can be gleaned from the type.
    #[must_use]
    pub fn any() -> Self {
        Self {
            bytes: Some(()),
            integer: Some(()),
            float: Some(()),
            boolean: Some(()),
            timestamp: Some(()),
            regex: Some(()),
            null: Some(()),
            array: Some(Collection::any()),
            object: Some(Collection::any()),
        }
    }

    /// The "bytes" type state.
    #[must_use]
    pub fn bytes() -> Self {
        Self {
            bytes: Some(()),
            integer: None,
            float: None,
            boolean: None,
            timestamp: None,
            regex: None,
            null: None,
            array: None,
            object: None,
        }
    }

    /// The "integer" type state.
    #[must_use]
    pub fn integer() -> Self {
        Self {
            bytes: None,
            integer: Some(()),
            float: None,
            boolean: None,
            timestamp: None,
            regex: None,
            null: None,
            array: None,
            object: None,
        }
    }

    /// The "float" type state.
    #[must_use]
    pub fn float() -> Self {
        Self {
            bytes: None,
            integer: None,
            float: Some(()),
            boolean: None,
            timestamp: None,
            regex: None,
            null: None,
            array: None,
            object: None,
        }
    }

    /// The "boolean" type state.
    #[must_use]
    pub fn boolean() -> Self {
        Self {
            bytes: None,
            integer: None,
            float: None,
            boolean: Some(()),
            timestamp: None,
            regex: None,
            null: None,
            array: None,
            object: None,
        }
    }

    /// The "timestamp" type state.
    #[must_use]
    pub fn timestamp() -> Self {
        Self {
            bytes: None,
            integer: None,
            float: None,
            boolean: None,
            timestamp: Some(()),
            regex: None,
            null: None,
            array: None,
            object: None,
        }
    }

    /// The "regex" type state.
    #[must_use]
    pub fn regex() -> Self {
        Self {
            bytes: None,
            integer: None,
            float: None,
            boolean: None,
            timestamp: None,
            regex: Some(()),
            null: None,
            array: None,
            object: None,
        }
    }

    /// The "null" type state.
    #[must_use]
    pub fn null() -> Self {
        Self {
            bytes: None,
            integer: None,
            float: None,
            boolean: None,
            timestamp: None,
            regex: None,
            null: Some(()),
            array: None,
            object: None,
        }
    }

    /// The "array" type state.
    #[must_use]
    pub fn array(map: impl Into<Collection<collection::Index>>) -> Self {
        Self {
            bytes: None,
            integer: None,
            float: None,
            boolean: None,
            timestamp: None,
            regex: None,
            null: None,
            array: Some(map.into()),
            object: None,
        }
    }

    /// The "object" type state.
    #[must_use]
    pub fn object(map: impl Into<Collection<collection::Field>>) -> Self {
        Self {
            bytes: None,
            integer: None,
            float: None,
            boolean: None,
            timestamp: None,
            regex: None,
            null: None,
            array: None,
            object: Some(map.into()),
        }
    }

    /// The "empty" state of a type.
    ///
    /// NOTE: We do NOT want to expose this state publicly, as its an invalid invariant to have
    ///       a type state with all variants set to "none".
    #[allow(unused)]
    fn empty() -> Self {
        Self {
            bytes: None,
            integer: None,
            float: None,
            boolean: None,
            timestamp: None,
            regex: None,
            null: None,
            array: None,
            object: None,
        }
    }
}

// `is_*` functions to check the state of a type.
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
    pub fn is_exact(&self) -> bool {
        let mut exact = None;

        exact = exact.xor(self.bytes);
        exact = exact.xor(self.integer);
        exact = exact.xor(self.float);
        exact = exact.xor(self.boolean);
        exact = exact.xor(self.timestamp);
        exact = exact.xor(self.regex);
        exact = exact.xor(self.null);
        exact = exact.xor(self.array.as_ref().map(|_| ()));
        exact = exact.xor(self.object.as_ref().map(|_| ()));
        exact.is_some()
    }
}

// `add_*` methods to extend the state of a type.
impl Kind {
    /// Add the `bytes` state to the type.
    ///
    /// If the type already included this state, the function returns `false`.
    pub fn add_bytes(&mut self) -> bool {
        self.bytes.replace(()).is_none()
    }

    /// Add the `integer` state to the type.
    ///
    /// If the type already included this state, the function returns `false`.
    pub fn add_integer(&mut self) -> bool {
        self.integer.replace(()).is_none()
    }

    /// Add the `float` state to the type.
    ///
    /// If the type already included this state, the function returns `false`.
    pub fn add_float(&mut self) -> bool {
        self.float.replace(()).is_none()
    }

    /// Add the `boolean` state to the type.
    ///
    /// If the type already included this state, the function returns `false`.
    pub fn add_boolean(&mut self) -> bool {
        self.boolean.replace(()).is_none()
    }

    /// Add the `timestamp` state to the type.
    ///
    /// If the type already included this state, the function returns `false`.
    pub fn add_timestamp(&mut self) -> bool {
        self.timestamp.replace(()).is_none()
    }

    /// Add the `regex` state to the type.
    ///
    /// If the type already included this state, the function returns `false`.
    pub fn add_regex(&mut self) -> bool {
        self.regex.replace(()).is_none()
    }

    /// Add the `null` state to the type.
    ///
    /// If the type already included this state, the function returns `false`.
    pub fn add_null(&mut self) -> bool {
        self.null.replace(()).is_none()
    }

    /// Add the `array` state to the type.
    ///
    /// If the type already included this state, the function returns `false`.
    pub fn add_array(&mut self, map: BTreeMap<collection::Index, Kind>) -> bool {
        self.array.replace(map.into()).is_none()
    }

    /// Add the `object` state to the type.
    ///
    /// If the type already included this state, the function returns `false`.
    pub fn add_object(&mut self, map: BTreeMap<collection::Field, Kind>) -> bool {
        self.object.replace(map.into()).is_none()
    }
}

// `remove_*` methods to narrow the state of a type.
impl Kind {
    /// Remove the `bytes` state from the type.
    ///
    /// If the type already excluded this state, the function returns `Ok(false)`.
    ///
    /// # Errors
    ///
    /// If removing this state leaves an "empty" type, then the error variant is returned. This was
    /// chosen, because when applying progressive type checking, there should _always_ be at least
    /// one state for a given type, the "no state left for a type" variant is a programming error.
    pub fn remove_bytes(&mut self) -> Result<bool, EmptyKindError> {
        if self.is_bytes() && self.is_exact() {
            return Err(EmptyKindError);
        }

        Ok(self.bytes.take().is_none())
    }

    /// Remove the `integer` state from the type.
    ///
    /// If the type already excluded this state, the function returns `Ok(false)`.
    ///
    /// # Errors
    ///
    /// If removing this state leaves an "empty" type, then the error variant is returned. This was
    /// chosen, because when applying progressive type checking, there should _always_ be at least
    /// one state for a given type, the "no state left for a type" variant is a programming error.
    pub fn remove_integer(&mut self) -> Result<bool, EmptyKindError> {
        if self.is_integer() && self.is_exact() {
            return Err(EmptyKindError);
        }

        Ok(self.integer.take().is_none())
    }

    /// Remove the `float` state from the type.
    ///
    /// If the type already excluded this state, the function returns `Ok(false)`.
    ///
    /// # Errors
    ///
    /// If removing this state leaves an "empty" type, then the error variant is returned. This was
    /// chosen, because when applying progressive type checking, there should _always_ be at least
    /// one state for a given type, the "no state left for a type" variant is a programming error.
    pub fn remove_float(&mut self) -> Result<bool, EmptyKindError> {
        if self.is_float() && self.is_exact() {
            return Err(EmptyKindError);
        }

        Ok(self.float.take().is_none())
    }

    /// Remove the `boolean` state from the type.
    ///
    /// If the type already excluded this state, the function returns `Ok(false)`.
    ///
    /// # Errors
    ///
    /// If removing this state leaves an "empty" type, then the error variant is returned. This was
    /// chosen, because when applying progressive type checking, there should _always_ be at least
    /// one state for a given type, the "no state left for a type" variant is a programming error.
    pub fn remove_boolean(&mut self) -> Result<bool, EmptyKindError> {
        if self.is_boolean() && self.is_exact() {
            return Err(EmptyKindError);
        }

        Ok(self.boolean.take().is_none())
    }

    /// Remove the `timestamp` state from the type.
    ///
    /// If the type already excluded this state, the function returns `Ok(false)`.
    ///
    /// # Errors
    ///
    /// If removing this state leaves an "empty" type, then the error variant is returned. This was
    /// chosen, because when applying progressive type checking, there should _always_ be at least
    /// one state for a given type, the "no state left for a type" variant is a programming error.
    pub fn remove_timestamp(&mut self) -> Result<bool, EmptyKindError> {
        if self.is_timestamp() && self.is_exact() {
            return Err(EmptyKindError);
        }

        Ok(self.timestamp.take().is_none())
    }

    /// Remove the `regex` state from the type.
    ///
    /// If the type already excluded this state, the function returns `Ok(false)`.
    ///
    /// # Errors
    ///
    /// If removing this state leaves an "empty" type, then the error variant is returned. This was
    /// chosen, because when applying progressive type checking, there should _always_ be at least
    /// one state for a given type, the "no state left for a type" variant is a programming error.
    pub fn remove_regex(&mut self) -> Result<bool, EmptyKindError> {
        if self.is_regex() && self.is_exact() {
            return Err(EmptyKindError);
        }

        Ok(self.regex.take().is_none())
    }

    /// Remove the `null` state from the type.
    ///
    /// If the type already excluded this state, the function returns `Ok(false)`.
    ///
    /// # Errors
    ///
    /// If removing this state leaves an "empty" type, then the error variant is returned. This was
    /// chosen, because when applying progressive type checking, there should _always_ be at least
    /// one state for a given type, the "no state left for a type" variant is a programming error.
    pub fn remove_null(&mut self) -> Result<bool, EmptyKindError> {
        if self.is_null() && self.is_exact() {
            return Err(EmptyKindError);
        }

        Ok(self.null.take().is_none())
    }

    /// Remove the `array` state from the type.
    ///
    /// If the type already excluded this state, the function returns `Ok(false)`.
    ///
    /// # Errors
    ///
    /// If removing this state leaves an "empty" type, then the error variant is returned. This was
    /// chosen, because when applying progressive type checking, there should _always_ be at least
    /// one state for a given type, the "no state left for a type" variant is a programming error.
    pub fn remove_array(&mut self) -> Result<bool, EmptyKindError> {
        if self.is_array() && self.is_exact() {
            return Err(EmptyKindError);
        }

        Ok(self.array.take().is_none())
    }

    /// Remove the `object` state from the type.
    ///
    /// If the type already excluded this state, the function returns `Ok(false)`.
    ///
    /// # Errors
    ///
    /// If removing this state leaves an "empty" type, then the error variant is returned. This was
    /// chosen, because when applying progressive type checking, there should _always_ be at least
    /// one state for a given type, the "no state left for a type" variant is a programming error.
    pub fn remove_object(&mut self) -> Result<bool, EmptyKindError> {
        if self.is_object() && self.is_exact() {
            return Err(EmptyKindError);
        }

        Ok(self.object.take().is_none())
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_any() {
            return f.write_str("any");
        }

        let mut kinds = vec![];

        if self.is_bytes() {
            kinds.push("string");
        }
        if self.is_integer() {
            kinds.push("integer");
        }
        if self.is_float() {
            kinds.push("float");
        }
        if self.is_boolean() {
            kinds.push("boolean");
        }
        if self.is_timestamp() {
            kinds.push("timestamp");
        }
        if self.is_regex() {
            kinds.push("regex");
        }
        if self.is_null() {
            kinds.push("null");
        }
        if self.is_array() {
            kinds.push("array");
        }
        if self.is_object() {
            kinds.push("object");
        }

        let last = kinds.remove(0);

        if kinds.is_empty() {
            return last.fmt(f);
        }

        let mut kinds = kinds.into_iter().peekable();

        while let Some(kind) = kinds.next() {
            kind.fmt(f)?;

            if kinds.peek().is_some() {
                f.write_str(", ")?;
            }
        }

        f.write_str(" or ")?;
        last.fmt(f)?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct EmptyKindError;

impl std::fmt::Display for EmptyKindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid empty type state variant")
    }
}

impl std::error::Error for EmptyKindError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nest_at_path() {
        // object
        let mut kind1 = Kind::integer();
        kind1 = kind1.nest_at_path(LookupBuf::from_str(".foo.bar").unwrap());

        let map1 = BTreeMap::from([("bar".into(), Kind::integer())]);
        let map2 = BTreeMap::from([("foo".into(), Kind::object(map1))]);
        let valid1 = Kind::object(map2);

        assert_eq!(kind1, valid1);

        // array
        let mut kind2 = Kind::boolean();
        kind2 = kind2.nest_at_path(LookupBuf::from_str(".foo[2]").unwrap());

        let map1 = BTreeMap::from([(2.into(), Kind::boolean())]);
        let map2 = BTreeMap::from([("foo".into(), Kind::array(map1))]);
        let valid2 = Kind::object(map2);

        assert_eq!(kind2, valid2)
    }
}
