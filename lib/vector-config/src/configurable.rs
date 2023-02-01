use crate::{GenerateError, Metadata};

/// A type that can be represented in a Vector configuration.
///
/// In Vector, we want to be able to generate a schema for our configuration such that we can have a Rust-agnostic
/// definition of exactly what is configurable, what values are allowed, what bounds exist, and so on and so forth.
///
/// `Configurable` provides the machinery to allow describing and encoding the shape of a type, recursively, so that by
/// instrumenting all transitive types of the configuration, the schema can be discovered by generating the schema from
/// some root type.
pub trait Configurable
where
    Self: Sized,
{
    /// Gets the referenceable name of this value, if any.
    ///
    /// When specified, this implies the value is both complex and standardized, and should be
    /// reused within any generated schema it is present in.
    fn referenceable_name() -> Option<&'static str> {
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
    fn is_optional() -> bool {
        false
    }

    /// Gets the metadata for this value.
    fn metadata() -> Metadata<Self> {
        Metadata::default()
    }

    /// Validates the given metadata against this type.
    ///
    /// This allows for validating specific aspects of the given metadata, such as validation
    /// bounds, and so on, to ensure they are valid for the given type. In some cases, such as with
    /// numeric types, there is a limited amount of validation that can occur within the
    /// `Configurable` derive macro, and additional validation must happen at runtime when the
    /// `Configurable` trait is being used, which this method allows for.
    fn validate_metadata(_metadata: &Metadata<Self>) -> Result<(), GenerateError> {
        Ok(())
    }

    /// Generates the schema for this value.
    ///
    /// # Errors
    ///
    /// If an error occurs while generating the schema, an error variant will be returned describing
    /// the issue.
    fn generate_schema(
        gen: &mut schemars::gen::SchemaGenerator,
    ) -> Result<schemars::schema::SchemaObject, GenerateError>;
}
