#![deny(missing_docs)]

use std::cell::RefCell;

use serde_json::Value;

use crate::{
    schema::{SchemaGenerator, SchemaObject},
    GenerateError, Metadata,
};

/// A type that can be represented in a Vector configuration.
///
/// In Vector, we want to be able to generate a schema for our configuration such that we can have a Rust-agnostic
/// definition of exactly what is configurable, what values are allowed, what bounds exist, and so on and so forth.
///
/// `Configurable` provides the machinery to allow describing and encoding the shape of a type, recursively, so that by
/// instrumenting all transitive types of the configuration, the schema can be discovered by generating the schema from
/// some root type.
pub trait Configurable {
    /// Gets the referenceable name of this value, if any.
    ///
    /// When specified, this implies the value is both complex and standardized, and should be
    /// reused within any generated schema it is present in.
    fn referenceable_name() -> Option<&'static str>
    where
        Self: Sized,
    {
        None
    }

    /// Whether or not this value is optional.
    ///
    /// This is specifically used to determine when a field is inherently optional, such as a field
    /// that is a true map like `HashMap<K, V>`. This doesn't apply to objects (i.e. structs)
    /// because structs are implicitly non-optional: they have a fixed shape and size, and so on.
    ///
    /// Maps, by definition, are inherently free-form, and thus inherently optional. Thus, this
    /// method should likely not be overridden except for implementing `Configurable` for map
    /// types. If you're using it for something else, you are expected to know what you're doing.
    fn is_optional() -> bool
    where
        Self: Sized,
    {
        false
    }

    /// Gets the metadata for this value.
    fn metadata() -> Metadata
    where
        Self: Sized,
    {
        Metadata::default()
    }

    /// Validates the given metadata against this type.
    ///
    /// This allows for validating specific aspects of the given metadata, such as validation
    /// bounds, and so on, to ensure they are valid for the given type. In some cases, such as with
    /// numeric types, there is a limited amount of validation that can occur within the
    /// `Configurable` derive macro, and additional validation must happen at runtime when the
    /// `Configurable` trait is being used, which this method allows for.
    fn validate_metadata(_metadata: &Metadata) -> Result<(), GenerateError>
    where
        Self: Sized,
    {
        Ok(())
    }

    /// Generates the schema for this value.
    ///
    /// # Errors
    ///
    /// If an error occurs while generating the schema, an error variant will be returned describing
    /// the issue.
    fn generate_schema(gen: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError>
    where
        Self: Sized;

    /// Create a new configurable reference table.
    fn as_configurable_ref() -> ConfigurableRef
    where
        Self: Sized + 'static,
    {
        ConfigurableRef::new::<Self>()
    }
}

/// A type that can be converted directly to a `serde_json::Value`. This is used when translating
/// the default value in a `Metadata` into a schema object.
pub trait ToValue {
    /// Convert this value into a `serde_json::Value`. Must not fail.
    fn to_value(&self) -> Value;
}

/// A pseudo-reference to a type that can be represented in a Vector configuration. This is
/// composed of references to all the class trait functions.
pub struct ConfigurableRef {
    // TODO: Turn this into a plain value once this is resolved:
    // https://github.com/rust-lang/rust/issues/63084
    type_name: fn() -> &'static str,
    // TODO: Turn this into a plain value once const trait functions are implemented
    // Ref: https://github.com/rust-lang/rfcs/pull/911
    referenceable_name: fn() -> Option<&'static str>,
    make_metadata: fn() -> Metadata,
    validate_metadata: fn(&Metadata) -> Result<(), GenerateError>,
    generate_schema: fn(&RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError>,
}

impl ConfigurableRef {
    /// Create a new configurable reference table.
    pub const fn new<T: Configurable + 'static>() -> Self {
        Self {
            type_name: std::any::type_name::<T>,
            referenceable_name: T::referenceable_name,
            make_metadata: T::metadata,
            validate_metadata: T::validate_metadata,
            generate_schema: T::generate_schema,
        }
    }

    pub(crate) fn type_name(&self) -> &'static str {
        (self.type_name)()
    }
    pub(crate) fn referenceable_name(&self) -> Option<&'static str> {
        (self.referenceable_name)()
    }
    pub(crate) fn make_metadata(&self) -> Metadata {
        (self.make_metadata)()
    }
    pub(crate) fn validate_metadata(&self, metadata: &Metadata) -> Result<(), GenerateError> {
        (self.validate_metadata)(metadata)
    }
    pub(crate) fn generate_schema(
        &self,
        gen: &RefCell<SchemaGenerator>,
    ) -> Result<SchemaObject, GenerateError> {
        (self.generate_schema)(gen)
    }
}
