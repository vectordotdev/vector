use crate::{
    schema::generate_number_schema,
    schemars::{gen::SchemaGenerator, schema::SchemaObject},
    Configurable, GenerateError, Metadata,
};

// Blanket implementation of `Configurable` for any `serde_with` helper that is also `Configurable`.
impl<T> Configurable for serde_with::As<T>
where
    T: Configurable,
{
    fn referenceable_name() -> Option<&'static str> {
        // Forward to the underlying `T`.
        T::referenceable_name()
    }

    fn description() -> Option<&'static str> {
        // Forward to the underlying `T`.
        T::description()
    }

    fn metadata() -> Metadata<Self> {
        // Forward to the underlying `T`.
        //
        // We have to convert from `Metadata<T>` to `Metadata<Self>` which erases the default value,
        // notably, but `serde_with` helpers should never actually have default values, so this is
        // essentially a no-op.
        T::metadata().convert::<Self>()
    }

    fn validate_metadata(metadata: &Metadata<Self>) -> Result<(), GenerateError> {
        // Forward to the underlying `T`.
        //
        // We have to convert from `Metadata<Self>` to `Metadata<T>` which erases the default value,
        // notably, but `serde_with` helpers should never actually have default values, so this is
        // essentially a no-op.
        let converted = metadata.convert::<T>();
        T::validate_metadata(&converted)
    }

    fn generate_schema(gen: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        // Forward to the underlying `T`.
        //
        // We have to convert from `Metadata<Self>` to `Metadata<T>` which erases the default value,
        // notably, but `serde_with` helpers should never actually have default values, so this is
        // essentially a no-op.
        T::generate_schema(gen)
    }
}

impl Configurable for serde_with::DurationSeconds<u64, serde_with::formats::Strict> {
    fn referenceable_name() -> Option<&'static str> {
        // We're masking the type parameters here because we only deal with whole seconds via this
        // version, and handle fractional seconds with `DurationSeconds<f64, Strict>`, which we
        // expose as `serde_with::DurationFractionalSeconds`.
        Some("serde_with::DurationSeconds")
    }

    fn description() -> Option<&'static str> {
        Some("A span of time, in whole seconds.")
    }

    fn generate_schema(_: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        // This boils down to a number schema, but we just need to shuttle around the metadata so
        // that we can call the relevant schema generation function.
        Ok(generate_number_schema::<u64>())
    }
}

impl Configurable for serde_with::DurationSeconds<f64, serde_with::formats::Strict> {
    fn referenceable_name() -> Option<&'static str> {
        // We're masking the type parameters here because we only deal with fractional seconds via this
        // version, and handle whole seconds with `DurationSeconds<u64, Strict>`, which we
        // expose as `serde_with::DurationSeconds`.
        Some("serde_with::DurationFractionalSeconds")
    }

    fn description() -> Option<&'static str> {
        Some("A span of time, in whole seconds.")
    }

    fn generate_schema(_: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        // This boils down to a number schema, but we just need to shuttle around the metadata so
        // that we can call the relevant schema generation function.
        Ok(generate_number_schema::<f64>())
    }
}
