use serde::{de, ser};
use serde_json::Value;
use std::{cell::RefCell, collections::BTreeSet};
use vector_lib::configurable::ToValue;

use indexmap::IndexMap;
use vector_lib::configurable::attributes::CustomAttribute;
use vector_lib::configurable::{
    schema::{
        apply_base_metadata, generate_one_of_schema, generate_struct_schema,
        get_or_generate_schema, SchemaGenerator, SchemaObject,
    },
    Configurable, GenerateError, Metadata,
};

use crate::sinks::util::buffer::compression::{
    generate_string_schema, CompressionLevel, ALGORITHM_NAME, ENUM_TAGGING_MODE, LEVEL_NAME,
};
use crate::sinks::util::Compression;

/// Compression configuration.
#[derive(Clone, Copy, Debug, Derivative, Eq, PartialEq)]
#[derivative(Default)]
pub enum ChronicleCompression {
    /// No compression.
    #[derivative(Default)]
    None,

    /// [Gzip][gzip] compression.
    ///
    /// [gzip]: https://www.gzip.org/
    Gzip(CompressionLevel),
}

impl From<ChronicleCompression> for Compression {
    fn from(compression: ChronicleCompression) -> Self {
        match compression {
            ChronicleCompression::None => Compression::None,
            ChronicleCompression::Gzip(compression_level) => Compression::Gzip(compression_level),
        }
    }
}

impl TryFrom<Compression> for ChronicleCompression {
    type Error = String;

    fn try_from(compression: Compression) -> Result<Self, Self::Error> {
        match compression {
            Compression::None => Ok(ChronicleCompression::None),
            Compression::Gzip(compression_level) => {
                Ok(ChronicleCompression::Gzip(compression_level))
            }
            _ => Err("Compression type is not supported by Chronicle".to_string()),
        }
    }
}

// Schema generation largely copied from `src/sinks/util/buffer/compression`
impl Configurable for ChronicleCompression {
    fn metadata() -> Metadata {
        Compression::metadata()
    }

    fn generate_schema(gen: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        // First, we'll create the string-only subschemas for each algorithm, and wrap those up
        // within a one-of schema.
        let mut string_metadata = Metadata::with_description("Compression algorithm.");
        string_metadata.add_custom_attribute(CustomAttribute::kv(ENUM_TAGGING_MODE, "external"));

        let none_string_subschema = generate_string_schema("None", None, "No compression.");
        let gzip_string_subschema = generate_string_schema(
            "Gzip",
            Some("[Gzip][gzip] compression."),
            "[gzip]: https://www.gzip.org/",
        );

        let mut all_string_oneof_subschema =
            generate_one_of_schema(&[none_string_subschema, gzip_string_subschema]);
        apply_base_metadata(&mut all_string_oneof_subschema, string_metadata);

        let compression_level_schema =
            get_or_generate_schema(&CompressionLevel::as_configurable_ref(), gen, None)?;

        let mut required = BTreeSet::new();
        required.insert(ALGORITHM_NAME.to_string());

        let mut properties = IndexMap::new();
        properties.insert(
            ALGORITHM_NAME.to_string(),
            all_string_oneof_subschema.clone(),
        );
        properties.insert(LEVEL_NAME.to_string(), compression_level_schema);

        let mut full_subschema = generate_struct_schema(properties, required, None);
        let mut full_metadata =
            Metadata::with_description("Compression algorithm and compression level.");
        full_metadata.add_custom_attribute(CustomAttribute::flag("docs::hidden"));
        apply_base_metadata(&mut full_subschema, full_metadata);

        Ok(generate_one_of_schema(&[
            all_string_oneof_subschema,
            full_subschema,
        ]))
    }
}

impl ToValue for ChronicleCompression {
    fn to_value(&self) -> Value {
        serde_json::to_value(Compression::from(*self))
            .expect("Could not convert compression settings to JSON")
    }
}

impl<'de> de::Deserialize<'de> for ChronicleCompression {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        Compression::deserialize(deserializer)
            .and_then(|x| ChronicleCompression::try_from(x).map_err(de::Error::custom))
    }
}

impl ser::Serialize for ChronicleCompression {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        Compression::serialize(&Compression::from(*self), serializer)
    }
}
