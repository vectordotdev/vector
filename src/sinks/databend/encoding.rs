use vector_lib::codecs::{encoding::SerializerConfig, CsvSerializerConfig, JsonSerializerConfig};
use vector_lib::configurable::configurable_component;

use crate::codecs::{EncodingConfig, Transformer};

/// Serializer configuration for Databend.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "codec", rename_all = "snake_case")]
#[configurable(metadata(docs::enum_tag_description = "The codec to use for encoding events."))]
pub(super) enum DatabendSerializerConfig {
    /// Encodes an event as a CSV message.
    ///
    /// This codec must be configured with fields to encode.
    ///
    Csv(
        /// Options for the CSV encoder.
        CsvSerializerConfig,
    ),

    /// Encodes an event as [JSON][json].
    ///
    /// [json]: https://www.json.org/
    Json(
        /// Encoding options specific to the Json serializer.
        JsonSerializerConfig,
    ),
}

impl From<DatabendSerializerConfig> for SerializerConfig {
    fn from(config: DatabendSerializerConfig) -> Self {
        match config {
            DatabendSerializerConfig::Csv(config) => Self::Csv(config),
            DatabendSerializerConfig::Json(config) => Self::Json(config),
        }
    }
}

impl Default for DatabendSerializerConfig {
    fn default() -> Self {
        Self::Json(JsonSerializerConfig::default())
    }
}

/// Encoding configuration for Databend.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[configurable(description = "Configures how events are encoded into raw bytes.")]
pub struct DatabendEncodingConfig {
    #[serde(flatten)]
    encoding: DatabendSerializerConfig,

    #[serde(flatten)]
    transformer: Transformer,
}

impl From<DatabendEncodingConfig> for EncodingConfig {
    fn from(encoding: DatabendEncodingConfig) -> Self {
        Self::new(encoding.encoding.into(), encoding.transformer)
    }
}

impl DatabendEncodingConfig {
    /// Get the encoding configuration.
    pub(super) const fn config(&self) -> &DatabendSerializerConfig {
        &self.encoding
    }
}

/// Defines how missing fields are handled for NDJson.
/// Refer to https://docs.databend.com/sql/sql-reference/file-format-options#null_field_as
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[configurable(metadata(docs::enum_tag_description = "How to handle missing fields for NDJson."))]
pub enum DatabendMissingFieldAS {
    /// Generates an error if a missing field is encountered.
    Error,

    /// Interprets missing fields as NULL values. An error will be generated for non-nullable fields.
    Null,

    /// Uses the default value of the field for missing fields.
    FieldDefault,

    /// Uses the default value of the field's data type for missing fields.
    TypeDefault,
}

impl Default for DatabendMissingFieldAS {
    fn default() -> Self {
        Self::Null
    }
}

impl DatabendMissingFieldAS {
    pub(super) const fn as_str(&self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Null => "NULL",
            Self::FieldDefault => "FIELD_DEFAULT",
            Self::TypeDefault => "TYPE_DEFAULT",
        }
    }
}
