use super::{Collection, Field, Index, Kind};

// Initializer functions.
impl Kind {
    /// The "empty" type state.
    ///
    /// This represents a state in which "no type matches the given value". In regular use, this is
    /// considered an invalid state caused by a programming error.
    ///
    /// It is useful for two purposes:
    ///
    /// 1. As extra validation to ensure such a state does not exist.
    /// 2. As a starting point to build up a new `Kind` with a valid state.
    ///
    /// Note that all other public methods of `Kind` prevent this state from happening. For
    /// example, the `remove_<state>` methods return an error if the to-be-removed state is the
    /// last state type present in `Kind`.
    #[must_use]
    pub const fn empty() -> Self {
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

    /// The "json" type state.
    ///
    /// This state is similar to `any`, except that it excludes any types that can't be represented
    /// in a native JSON-type (such as `timestamp` and `regex`).
    #[must_use]
    pub fn json() -> Self {
        Self {
            bytes: Some(()),
            integer: Some(()),
            float: Some(()),
            boolean: Some(()),
            timestamp: None,
            regex: None,
            null: Some(()),
            array: Some(Collection::json()),
            object: Some(Collection::json()),
        }
    }

    /// The "primitive" type state.
    ///
    /// This state represents all types, _except_ ones that contain collection of types (e.g.
    /// objects and arrays).
    #[must_use]
    pub const fn primitive() -> Self {
        Self {
            bytes: Some(()),
            integer: Some(()),
            float: Some(()),
            boolean: Some(()),
            timestamp: Some(()),
            regex: Some(()),
            null: Some(()),
            array: None,
            object: None,
        }
    }

    /// The "bytes" type state.
    #[must_use]
    pub const fn bytes() -> Self {
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
    pub const fn integer() -> Self {
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
    pub const fn float() -> Self {
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
    pub const fn boolean() -> Self {
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
    pub const fn timestamp() -> Self {
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
    pub const fn regex() -> Self {
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
    pub const fn null() -> Self {
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
    pub fn array(collection: impl Into<Collection<Index>>) -> Self {
        Self {
            bytes: None,
            integer: None,
            float: None,
            boolean: None,
            timestamp: None,
            regex: None,
            null: None,
            array: Some(collection.into()),
            object: None,
        }
    }

    /// The "object" type state.
    #[must_use]
    pub fn object(collection: impl Into<Collection<Field>>) -> Self {
        Self {
            bytes: None,
            integer: None,
            float: None,
            boolean: None,
            timestamp: None,
            regex: None,
            null: None,
            array: None,
            object: Some(collection.into()),
        }
    }
}

// `or_*` methods to extend the state of a type using a builder-like API.
impl Kind {
    /// Add the `bytes` state to the type.
    #[must_use]
    pub const fn or_bytes(mut self) -> Self {
        self.bytes = Some(());
        self
    }

    /// Add the `integer` state to the type.
    #[must_use]
    pub const fn or_integer(mut self) -> Self {
        self.integer = Some(());
        self
    }

    /// Add the `float` state to the type.
    #[must_use]
    pub const fn or_float(mut self) -> Self {
        self.float = Some(());
        self
    }

    /// Add the `boolean` state to the type.
    #[must_use]
    pub const fn or_boolean(mut self) -> Self {
        self.boolean = Some(());
        self
    }

    /// Add the `timestamp` state to the type.
    #[must_use]
    pub const fn or_timestamp(mut self) -> Self {
        self.timestamp = Some(());
        self
    }

    /// Add the `regex` state to the type.
    #[must_use]
    pub const fn or_regex(mut self) -> Self {
        self.regex = Some(());
        self
    }

    /// Add the `null` state to the type.
    #[must_use]
    pub const fn or_null(mut self) -> Self {
        self.null = Some(());
        self
    }

    /// Add the `array` state to the type.
    #[must_use]
    pub fn or_array(mut self, collection: impl Into<Collection<Index>>) -> Self {
        self.array = Some(collection.into());
        self
    }

    /// Add the `object` state to the type.
    #[must_use]
    pub fn or_object(mut self, collection: impl Into<Collection<Field>>) -> Self {
        self.object = Some(collection.into());
        self
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
    pub fn add_array(&mut self, collection: impl Into<Collection<Index>>) -> bool {
        self.array.replace(collection.into()).is_none()
    }

    /// Add the `object` state to the type.
    ///
    /// If the type already included this state, the function returns `false`.
    pub fn add_object(&mut self, collection: impl Into<Collection<Field>>) -> bool {
        self.object.replace(collection.into()).is_none()
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
    /// If removing this state would leave an "empty" type, then the error variant is returned.
    /// This was chosen, because when applying progressive type checking, there should _always_ be
    /// at least one state for a given type, the "no state left for a type" variant is
    /// a programming error.
    pub fn remove_bytes(&mut self) -> Result<bool, EmptyKindError> {
        if self.is_bytes() {
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
    /// If removing this state would leave an "empty" type, then the error variant is returned.
    /// This was chosen, because when applying progressive type checking, there should _always_ be
    /// at least one state for a given type, the "no state left for a type" variant is
    /// a programming error.
    pub fn remove_integer(&mut self) -> Result<bool, EmptyKindError> {
        if self.is_integer() {
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
    /// If removing this state would leave an "empty" type, then the error variant is returned.
    /// This was chosen, because when applying progressive type checking, there should _always_ be
    /// at least one state for a given type, the "no state left for a type" variant is
    /// a programming error.
    pub fn remove_float(&mut self) -> Result<bool, EmptyKindError> {
        if self.is_float() {
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
    /// If removing this state would leave an "empty" type, then the error variant is returned.
    /// This was chosen, because when applying progressive type checking, there should _always_ be
    /// at least one state for a given type, the "no state left for a type" variant is
    /// a programming error.
    pub fn remove_boolean(&mut self) -> Result<bool, EmptyKindError> {
        if self.is_boolean() {
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
    /// If removing this state would leave an "empty" type, then the error variant is returned.
    /// This was chosen, because when applying progressive type checking, there should _always_ be
    /// at least one state for a given type, the "no state left for a type" variant is
    /// a programming error.
    pub fn remove_timestamp(&mut self) -> Result<bool, EmptyKindError> {
        if self.is_timestamp() {
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
    /// If removing this state would leave an "empty" type, then the error variant is returned.
    /// This was chosen, because when applying progressive type checking, there should _always_ be
    /// at least one state for a given type, the "no state left for a type" variant is
    /// a programming error.
    pub fn remove_regex(&mut self) -> Result<bool, EmptyKindError> {
        if self.is_regex() {
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
    /// If removing this state would leave an "empty" type, then the error variant is returned.
    /// This was chosen, because when applying progressive type checking, there should _always_ be
    /// at least one state for a given type, the "no state left for a type" variant is
    /// a programming error.
    pub fn remove_null(&mut self) -> Result<bool, EmptyKindError> {
        if self.is_null() {
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
    /// If removing this state would leave an "empty" type, then the error variant is returned.
    /// This was chosen, because when applying progressive type checking, there should _always_ be
    /// at least one state for a given type, the "no state left for a type" variant is
    /// a programming error.
    pub fn remove_array(&mut self) -> Result<bool, EmptyKindError> {
        if self.is_array() {
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
    /// If removing this state would leave an "empty" type, then the error variant is returned.
    /// This was chosen, because when applying progressive type checking, there should _always_ be
    /// at least one state for a given type, the "no state left for a type" variant is
    /// a programming error.
    pub fn remove_object(&mut self) -> Result<bool, EmptyKindError> {
        if self.is_object() {
            return Err(EmptyKindError);
        }

        Ok(self.object.take().is_none())
    }
}

/// The error triggered by any of [`Kind`]s `remove_*` methods, if the call to that method would
/// leave the `Kind` in an empty state.
#[derive(Debug)]
pub struct EmptyKindError;

impl std::fmt::Display for EmptyKindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid empty type state variant")
    }
}

impl std::error::Error for EmptyKindError {}
