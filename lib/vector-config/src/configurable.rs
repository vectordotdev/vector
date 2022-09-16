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

    /// Gets the human-readable description of this value, if any.
    ///
    /// For standard types, this will be `None`. Commonly, custom types would implement this
    /// directly, while fields using standard types would provide a field-specific description that
    /// would be used instead of the default descrption.
    fn description() -> Option<&'static str> {
        None
    }

    /// Whether or not this value is optional.
    fn is_optional() -> bool {
        false
    }

    /// Gets the metadata for this value.
    fn metadata() -> Metadata<Self> {
        let mut metadata = Metadata::default();
        if let Some(description) = Self::description() {
            metadata.set_description(description);
        }
        metadata
    }

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
