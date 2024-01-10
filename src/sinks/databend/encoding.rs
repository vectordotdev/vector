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
