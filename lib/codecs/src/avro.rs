//! Contains common definitions for Avro codec support

use vector_config::configurable_component;

/// Specifies the Avro encoding format for serialization/deserialization.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AvroEncoding {
    /// Single-object datum encoding.
    ///
    /// Each event is encoded/decoded as a standalone Avro datum without container metadata.
    /// This is the default encoding for backward compatibility.
    #[default]
    Datum,

    /// Object Container File encoding.
    ///
    /// For serialization: Writes data in Avro Object Container File format, which embeds the schema
    /// in the output and organizes records into data blocks. Suitable for file-based storage.
    ///
    /// For deserialization: Reads from Avro Object Container File format, extracting multiple records.
    ObjectContainerFile,
}

/// Specifies how to handle the Avro schema for Object Container File decoding.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AvroSchemaSource {
    /// Use the schema provided in the configuration.
    /// For OCF, validate that the embedded schema matches the provided one.
    #[default]
    Provided,

    /// Use the schema embedded in the OCF file.
    /// The provided schema is ignored.
    Embedded,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json;
    #[test]
    fn deserializes_object_container_file() {
        #[derive(Deserialize)]
        struct Cfg {
            encoding: AvroEncoding,
        }

        let cfg: Cfg = serde_json::from_str(r#"{"encoding": "object_container_file"}"#).unwrap();
        assert_eq!(cfg.encoding, AvroEncoding::ObjectContainerFile);
    }
}
