#[derive(Debug)]
pub enum BoundDirection {
    /// Minimum bound.
    Minimum,

    /// Maximum bound.
    Maximum,
}

/// Schema generation error.
#[derive(Debug)]
pub enum GenerateError {
    /// An invalid type was encountered during schema generation.
    ///
    /// This typically means that the type cannot ever be represented correctly in a generated
    /// schema, and so has been hard-coded to always fail during schema generation.
    ///
    /// An example of such an implementation would be the unit type.
    InvalidType,

    /// A numeric bound, specified in a validation attribute, is invalid for the underlying type.
    IncompatibleNumericBounds {
        /// The name of the numeric type. (e.g. `u64`)
        numeric_type: &'static str,

        /// The bound direction that was exceeded.
        bound_direction: BoundDirection,

        /// The value of the mechanical bound on the underlying type.
        ///
        /// This is the inherent limit, such as a 8-bit integer only being able to go as high as
        /// 2^8.
        mechanical_bound: f64,

        /// The value of the specified bound.
        ///
        /// This is the value specified in the validation attribute itself.
        specified_bound: f64,
    },

    /// A type that is not string-like was specified as the key type for a map.
    ///
    /// As maps resolve to the equivalent of a JSON object, which requires strings for properties
    /// (i.e. the key), we can only allow types to be used as the key of a map when their schema
    /// maps to a plain string.
    MapKeyNotStringLike {
        /// The name of the key type. (e.g. `bool`)
        key_type: &'static str,

        /// The name of the map type. (e.g. `HashMap<bool, String>`)
        ///
        /// This is primarily for diagnostic purposes, to determine what map usage is actually
        /// responsible for the error. As the error occurs at runtime, we have limited information
        /// to point the caller directly to the file/line where the misusage is occurring, other
        /// than the type name itself.
        map_type: &'static str,
    },

    /// A type tried to modify a schema to be optional, but provided an invalid schema.
    ///
    /// In order to make a schema "optional", which implies allowing it to match `null`, it must not
    /// be a schema reference and it must already have an instance type, or types, defined.
    InvalidOptionalSchema,
}
