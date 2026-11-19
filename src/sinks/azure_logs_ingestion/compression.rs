use std::{cell::RefCell, collections::BTreeSet};

use indexmap::IndexMap;
use serde::{de, ser};
use serde_json::Value;
use vector_lib::configurable::{
    Configurable, GenerateError, Metadata, ToValue,
    attributes::CustomAttribute,
    schema::{
        SchemaGenerator, SchemaObject, apply_metadata, generate_one_of_schema,
        generate_struct_schema, get_or_generate_schema,
    },
};

use crate::sinks::util::{
    Compression,
    buffer::compression::{
        ALGORITHM_NAME, CompressionLevel, ENUM_TAGGING_MODE, LEVEL_NAME, generate_string_schema,
    },
};

/// Compression configuration.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AzureLogsIngestionCompression {
    /// No compression.
    #[default]
    None,

    /// [Gzip][gzip] compression.
    ///
    /// [gzip]: https://www.gzip.org/
    Gzip(CompressionLevel),
}

impl From<AzureLogsIngestionCompression> for Compression {
    fn from(compression: AzureLogsIngestionCompression) -> Self {
        match compression {
            AzureLogsIngestionCompression::None => Compression::None,
            AzureLogsIngestionCompression::Gzip(compression_level) => {
                Compression::Gzip(compression_level)
            }
        }
    }
}

impl TryFrom<Compression> for AzureLogsIngestionCompression {
    type Error = String;

    fn try_from(compression: Compression) -> Result<Self, Self::Error> {
        match compression {
            Compression::None => Ok(AzureLogsIngestionCompression::None),
            Compression::Gzip(compression_level) => {
                Ok(AzureLogsIngestionCompression::Gzip(compression_level))
            }
            _ => Err("Azure Logs Ingestion supports only none or gzip compression".to_string()),
        }
    }
}

// Schema generation largely copied from `src/sinks/util/buffer/compression`
impl Configurable for AzureLogsIngestionCompression {
    fn metadata() -> Metadata {
        Compression::metadata()
    }

    fn generate_schema(
        generator: &RefCell<SchemaGenerator>,
    ) -> Result<SchemaObject, GenerateError> {
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
        apply_metadata(&mut all_string_oneof_subschema, string_metadata);

        let compression_level_schema =
            get_or_generate_schema(&CompressionLevel::as_configurable_ref(), generator, None)?;

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
        apply_metadata(&mut full_subschema, full_metadata);

        Ok(generate_one_of_schema(&[
            all_string_oneof_subschema,
            full_subschema,
        ]))
    }
}

impl ToValue for AzureLogsIngestionCompression {
    fn to_value(&self) -> Value {
        serde_json::to_value(Compression::from(*self))
            .expect("Could not convert compression settings to JSON")
    }
}

impl<'de> de::Deserialize<'de> for AzureLogsIngestionCompression {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        Compression::deserialize(deserializer)
            .and_then(|x| AzureLogsIngestionCompression::try_from(x).map_err(de::Error::custom))
    }
}

impl ser::Serialize for AzureLogsIngestionCompression {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        Compression::serialize(&Compression::from(*self), serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_supported_compression() {
        for value in [
            serde_json::json!("none"),
            serde_json::json!("gzip"),
            serde_json::json!({"algorithm": "gzip", "level": 0}),
            serde_json::json!({"algorithm": "gzip", "level": 9}),
        ] {
            let compression: AzureLogsIngestionCompression = serde_json::from_value(value).unwrap();
            let serialized = serde_json::to_value(compression).unwrap();
            assert_eq!(
                serde_json::from_value::<AzureLogsIngestionCompression>(serialized).unwrap(),
                compression
            );
        }
    }

    #[test]
    fn rejects_unsupported_compression_and_levels() {
        for value in [
            serde_json::json!("snappy"),
            serde_json::json!({"algorithm": "zstd", "level": 1}),
            serde_json::json!({"algorithm": "zlib", "level": 1}),
            serde_json::json!({"algorithm": "gzip", "level": 10}),
        ] {
            assert!(serde_json::from_value::<AzureLogsIngestionCompression>(value).is_err());
        }
    }
}
