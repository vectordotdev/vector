use super::{Collection, Field, Index, Kind};

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
            undefined: Some(()),
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
            undefined: None,
            array: Some(Collection::json()),
            object: Some(Collection::json()),
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
            undefined: None,
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
            undefined: None,
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
            undefined: None,
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
            undefined: None,
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
            undefined: None,
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
            undefined: None,
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
            undefined: None,
            array: None,
            object: None,
        }
    }

    /// The "undefined" type state.
    #[must_use]
    pub const fn undefined() -> Self {
        Self {
            bytes: None,
            integer: None,
            float: None,
            boolean: None,
            timestamp: None,
            regex: None,
            null: None,
            undefined: Some(()),
            array: None,
            object: None,
        }
    }

    /// The "never" type state.
    #[must_use]
    pub const fn never() -> Self {
        Self {
            bytes: None,
            integer: None,
            float: None,
            boolean: None,
            timestamp: None,
            regex: None,
            null: None,
            undefined: None,
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
            undefined: None,
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
            undefined: None,
            array: None,
            object: Some(collection.into()),
        }
    }

    /// An object that can have any fields.
    #[must_use]
    pub fn any_object() -> Self {
        Self::object(Collection::any())
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

    /// Add the `undefined` state to the type.
    #[must_use]
    pub const fn or_undefined(mut self) -> Self {
        self.undefined = Some(());
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

    /// Add the `null` state to the type.
    ///
    /// If the type already included this state, the function returns `false`.
    pub fn add_undefined(&mut self) -> bool {
        self.undefined.replace(()).is_none()
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
    /// If the type previously included this state, true is returned.
    pub fn remove_bytes(&mut self) -> bool {
        self.bytes.take().is_some()
    }

    /// Remove the `integer` state from the type.
    ///
    /// If the type previously included this state, true is returned.
    pub fn remove_integer(&mut self) -> bool {
        self.integer.take().is_some()
    }

    /// Remove the `float` state from the type.
    ///
    /// If the type previously included this state, true is returned.
    pub fn remove_float(&mut self) -> bool {
        self.float.take().is_some()
    }

    /// Remove the `boolean` state from the type.
    ///
    /// If the type previously included this state, true is returned.
    pub fn remove_boolean(&mut self) -> bool {
        self.boolean.take().is_some()
    }

    /// Remove the `timestamp` state from the type.
    ///
    /// If the type previously included this state, true is returned.
    pub fn remove_timestamp(&mut self) -> bool {
        self.timestamp.take().is_some()
    }

    /// Remove the `regex` state from the type.
    ///
    /// If the type previously included this state, true is returned.
    pub fn remove_regex(&mut self) -> bool {
        self.regex.take().is_some()
    }

    /// Remove the `null` state from the type.
    ///
    /// If the type previously included this state, true is returned.
    pub fn remove_null(&mut self) -> bool {
        self.null.take().is_some()
    }

    /// Remove the `undefined` state from the type.
    ///
    /// If the type previously included this state, true is returned.
    pub fn remove_undefined(&mut self) -> bool {
        self.undefined.take().is_some()
    }

    /// Remove the `array` state from the type.
    ///
    /// If the type previously included this state, true is returned.
    pub fn remove_array(&mut self) -> bool {
        self.array.take().is_some()
    }

    /// Remove the `object` state from the type.
    ///
    /// If the type previously included this state, true is returned.
    pub fn remove_object(&mut self) -> bool {
        self.object.take().is_some()
    }
}

// `without_*` methods to narrow the state of a type (functional).
impl Kind {
    /// Remove the `undefined` state from the type, and return it.
    #[must_use]
    pub fn without_undefined(&self) -> Self {
        let mut kind = self.clone();
        kind.remove_undefined();
        kind
    }

    /// Remove the `array` state from the type, and return it.
    #[must_use]
    pub fn without_array(&self) -> Self {
        let mut kind = self.clone();
        kind.remove_array();
        kind
    }

    /// Remove the `object` state from the type, and return it.
    #[must_use]
    pub fn without_object(&self) -> Self {
        let mut kind = self.clone();
        kind.remove_object();
        kind
    }
}
