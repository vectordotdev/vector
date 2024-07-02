use std::cell::RefCell;

use vector_config_common::{attributes::CustomAttribute, constants};

use crate::schema::generate_optional_schema;
use crate::{
    num::NumberClass,
    schema::{generate_number_schema, SchemaGenerator, SchemaObject},
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

    fn metadata() -> Metadata {
        // Forward to the underlying `T`.
        //
        // We have to convert from `Metadata` to `Metadata` which erases the default value,
        // notably, but `serde_with` helpers should never actually have default values, so this is
        // essentially a no-op.
        T::metadata().convert()
    }

    fn validate_metadata(metadata: &Metadata) -> Result<(), GenerateError> {
        // Forward to the underlying `T`.
        //
        // We have to convert from `Metadata` to `Metadata` which erases the default value,
        // notably, but `serde_with` helpers should never actually have default values, so this is
        // essentially a no-op.
        let converted = metadata.convert();
        T::validate_metadata(&converted)
    }

    fn generate_schema(gen: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        // Forward to the underlying `T`.
        //
        // We have to convert from `Metadata` to `Metadata` which erases the default value,
        // notably, but `serde_with` helpers should never actually have default values, so this is
        // essentially a no-op.
        T::generate_schema(gen)
    }
}

impl Configurable for serde_with::DurationSeconds<u64, serde_with::formats::Strict> {
    fn referenceable_name() -> Option<&'static str> {
        // We're masking the type parameters here because we only deal with whole seconds via this
        // version, and handle fractional seconds with `DurationSecondsWithFrac<f64, Strict>`, which we
        // expose as `serde_with::DurationFractionalSeconds`.
        Some("serde_with::DurationSeconds")
    }

    fn metadata() -> Metadata {
        let mut metadata = Metadata::default();
        metadata.set_description("A span of time, in whole seconds.");
        metadata.add_custom_attribute(CustomAttribute::kv(
            constants::DOCS_META_NUMERIC_TYPE,
            NumberClass::Unsigned,
        ));
        metadata.add_custom_attribute(CustomAttribute::kv(
            constants::DOCS_META_TYPE_UNIT,
            "seconds",
        ));
        metadata
    }

    fn generate_schema(_: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        // This boils down to a number schema, but we just need to shuttle around the metadata so
        // that we can call the relevant schema generation function.
        Ok(generate_number_schema::<u64>())
    }
}

impl Configurable for serde_with::DurationSecondsWithFrac<f64, serde_with::formats::Strict> {
    fn referenceable_name() -> Option<&'static str> {
        // We're masking the type parameters here because we only deal with fractional seconds via this
        // version, and handle whole seconds with `DurationSeconds<u64, Strict>`, which we
        // expose as `serde_with::DurationSeconds`.
        Some("serde_with::DurationFractionalSeconds")
    }

    fn metadata() -> Metadata {
        let mut metadata = Metadata::default();
        metadata.set_description("A span of time, in fractional seconds.");
        metadata.add_custom_attribute(CustomAttribute::kv(
            constants::DOCS_META_NUMERIC_TYPE,
            NumberClass::FloatingPoint,
        ));
        metadata.add_custom_attribute(CustomAttribute::kv(
            constants::DOCS_META_TYPE_UNIT,
            "seconds",
        ));
        metadata
    }

    fn generate_schema(_: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        // This boils down to a number schema, but we just need to shuttle around the metadata so
        // that we can call the relevant schema generation function.
        Ok(generate_number_schema::<f64>())
    }
}

impl Configurable for serde_with::DurationMilliSeconds<u64, serde_with::formats::Strict> {
    fn referenceable_name() -> Option<&'static str> {
        // We're masking the type parameters here because we only deal with whole milliseconds via this
        // version.
        Some("serde_with::DurationMilliSeconds")
    }

    fn metadata() -> Metadata {
        let mut metadata = Metadata::default();
        metadata.set_description("A span of time, in whole milliseconds.");
        metadata.add_custom_attribute(CustomAttribute::kv(
            constants::DOCS_META_NUMERIC_TYPE,
            NumberClass::Unsigned,
        ));
        metadata.add_custom_attribute(CustomAttribute::kv(
            constants::DOCS_META_TYPE_UNIT,
            "milliseconds",
        ));
        metadata
    }

    fn generate_schema(_: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        // This boils down to a number schema, but we just need to shuttle around the metadata so
        // that we can call the relevant schema generation function.
        Ok(generate_number_schema::<u64>())
    }
}

impl Configurable for Option<serde_with::DurationMilliSeconds<u64, serde_with::formats::Strict>> {
    fn generate_schema(gen: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError>
    where
        Self: Sized,
    {
        generate_optional_schema(&u64::as_configurable_ref(), gen)
    }
}
