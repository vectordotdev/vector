use serde::{Deserialize, Deserializer, de};
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

/// Enables deserializing compression from a bool (legacy) or string (new).
///
/// For backward compatibility:
/// - `true` maps to `VectorCompression::Gzip`
/// - `false` maps to `VectorCompression::None`
///
/// New syntax:
/// - `"none"`, `"gzip"`, `"zstd"` as strings
pub fn bool_or_vector_compression<'de, D>(deserializer: D) -> Result<VectorCompression, D::Error>
where
    D: Deserializer<'de>,
{
    struct BoolOrVectorCompression;

    impl<'de> de::Visitor<'de> for BoolOrVectorCompression {
        type Value = VectorCompression;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("boolean (deprecated) or string (\"none\", \"gzip\", or \"zstd\")")
        }

        fn visit_bool<E>(self, value: bool) -> Result<VectorCompression, E>
        where
            E: de::Error,
        {
            if value {
                Ok(VectorCompression::Gzip)
            } else {
                Ok(VectorCompression::None)
            }
        }

        fn visit_str<E>(self, value: &str) -> Result<VectorCompression, E>
        where
            E: de::Error,
        {
            VectorCompression::deserialize(de::value::StrDeserializer::new(value))
        }
    }

    deserializer.deserialize_any(BoolOrVectorCompression)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct TestConfig {
        #[serde(deserialize_with = "bool_or_vector_compression")]
        compression: VectorCompression,
    }

    #[test]
    fn test_legacy_true() {
        let json = r#"{"compression": true}"#;
        let result: TestConfig = serde_json::from_str(json).unwrap();
        assert_eq!(result.compression, VectorCompression::Gzip);
    }

    #[test]
    fn test_legacy_false() {
        let json = r#"{"compression": false}"#;
        let result: TestConfig = serde_json::from_str(json).unwrap();
        assert_eq!(result.compression, VectorCompression::None);
    }

    #[test]
    fn test_string_gzip() {
        let json = r#"{"compression": "gzip"}"#;
        let result: TestConfig = serde_json::from_str(json).unwrap();
        assert_eq!(result.compression, VectorCompression::Gzip);
    }

    #[test]
    fn test_string_zstd() {
        let json = r#"{"compression": "zstd"}"#;
        let result: TestConfig = serde_json::from_str(json).unwrap();
        assert_eq!(result.compression, VectorCompression::Zstd);
    }

    #[test]
    fn test_string_none() {
        let json = r#"{"compression": "none"}"#;
        let result: TestConfig = serde_json::from_str(json).unwrap();
        assert_eq!(result.compression, VectorCompression::None);
    }

    #[test]
    fn test_unsupported_algorithm_rejected() {
        let json = r#"{"compression": "snappy"}"#;
        let result = serde_json::from_str::<TestConfig>(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_object_syntax_rejected() {
        let json = r#"{"compression": {"algorithm": "zstd", "level": 3}}"#;
        let result = serde_json::from_str::<TestConfig>(json);
        assert!(result.is_err());
    }
}
