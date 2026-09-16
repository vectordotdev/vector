use vector_lib::configurable::configurable_component;

/// Compression configuration for the Vector sink.
///
/// Only `gzip` and `zstd` are supported as compression algorithms for the
/// Vector sink's gRPC transport. Compression levels are not configurable
/// as the underlying tonic library does not support them.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
#[configurable(metadata(
    docs::enum_tag_description = "The compression algorithm to use for sending."
))]
pub enum VectorCompression {
    /// No compression.
    #[default]
    None,

    /// [Gzip][gzip] compression.
    ///
    /// [gzip]: https://www.gzip.org/
    Gzip,

    /// [Zstandard][zstd] compression.
    ///
    /// [zstd]: https://facebook.github.io/zstd/
    Zstd,
}

impl VectorCompression {
    /// Returns the corresponding `tonic::codec::CompressionEncoding`, if any.
    pub const fn as_tonic_encoding(self) -> Option<tonic::codec::CompressionEncoding> {
        match self {
            VectorCompression::None => Option::None,
            VectorCompression::Gzip => Some(tonic::codec::CompressionEncoding::Gzip),
            VectorCompression::Zstd => Some(tonic::codec::CompressionEncoding::Zstd),
        }
    }
}
