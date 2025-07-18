use vector_lib::configurable::configurable_component;

/// Compression configuration.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(rename_all = "snake_case")]
#[configurable(metadata(
    docs::enum_tag_description = "The compression algorithm to use for sending."
))]
pub enum DatabendCompression {
    /// No compression.
    None,

    /// [Gzip][gzip] compression.
    ///
    /// [gzip]: https://www.gzip.org/
    Gzip,
}

impl Default for DatabendCompression {
    fn default() -> Self {
        Self::None
    }
}
