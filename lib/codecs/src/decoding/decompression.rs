//! Decompression of complete message payloads before framing and deserializing.
//!
//! Some producers compress each message payload at the application level (as opposed to
//! transport-level compression, which is handled transparently by the client library or protocol
//! layer). Sources that receive complete message payloads (e.g. `kafka`) can use
//! [`DecompressionConfig`] to decompress each payload before it enters the framing / decoding
//! pipeline.

use std::{
    fmt::Debug,
    io::{self, Cursor},
    path::PathBuf,
    sync::Arc,
};

use vector_common::decompression::{CappedDecoder, DecoderDictionary};
use vector_config::configurable_component;

/// Algorithm used to decompress message payloads.
#[configurable_component]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DecompressionAlgorithm {
    /// [Gzip][gzip] decompression.
    ///
    /// [gzip]: https://www.gzip.org/
    Gzip,

    /// [Zlib][zlib] decompression.
    ///
    /// [zlib]: https://zlib.net/
    Zlib,

    /// [Zstandard][zstd] decompression.
    ///
    /// [zstd]: https://facebook.github.io/zstd/
    Zstd,
}

impl DecompressionAlgorithm {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Gzip => "gzip",
            Self::Zlib => "zlib",
            Self::Zstd => "zstd",
        }
    }
}

/// Configuration for decompressing message payloads.
///
/// This applies to compression performed by the producer on each message payload, before the
/// payload was sent. Transport-level compression is handled by the protocol layer and does not
/// require this option.
///
/// Payloads are decompressed before framing and decoding are applied.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct DecompressionConfig {
    /// The decompression algorithm.
    pub algorithm: DecompressionAlgorithm,

    /// The path to a compression dictionary to use when decompressing payloads.
    ///
    /// The dictionary must be the same as the one used by the producer when compressing the
    /// payloads. Only supported with the `zstd` algorithm.
    #[serde(default, skip_serializing_if = "vector_core::serde::is_default")]
    #[configurable(metadata(docs::examples = "/etc/vector/compression.dict"))]
    pub dictionary_path: Option<PathBuf>,
}

impl DecompressionConfig {
    /// Builds a [`Decompressor`] from this configuration.
    ///
    /// The dictionary, if configured, is read and prepared once here so that per-payload
    /// decompression can reference it cheaply.
    ///
    /// # Errors
    ///
    /// Returns an error if a dictionary is configured with a non-`zstd` algorithm, or if the
    /// dictionary file cannot be read.
    pub fn build(&self) -> vector_common::Result<Decompressor> {
        let dictionary = match (&self.dictionary_path, self.algorithm) {
            (None, _) => None,
            (Some(path), DecompressionAlgorithm::Zstd) => {
                let contents = std::fs::read(path).map_err(|error| {
                    format!(
                        "Failed to read decompression dictionary file {}: {error}.",
                        path.display()
                    )
                })?;
                Some(Arc::new(DecoderDictionary::copy(&contents)))
            }
            (Some(_), algorithm) => {
                return Err(format!(
                    "`dictionary_path` is not supported with the `{}` algorithm, only `zstd`.",
                    algorithm.as_str()
                )
                .into());
            }
        };

        Ok(Decompressor {
            algorithm: self.algorithm,
            dictionary,
        })
    }
}

/// Decompresses complete message payloads, built from a [`DecompressionConfig`].
#[derive(Clone)]
pub struct Decompressor {
    algorithm: DecompressionAlgorithm,
    dictionary: Option<Arc<DecoderDictionary<'static>>>,
}

impl Debug for Decompressor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decompressor")
            .field("algorithm", &self.algorithm)
            .field("dictionary", &self.dictionary.as_ref().map(|_| "<dict>"))
            .finish()
    }
}

impl Decompressor {
    /// Decompresses a complete message payload.
    ///
    /// The decompressed size is bounded by the globally configured cap (see
    /// `vector_common::decompression`), so a malformed or malicious payload cannot drive
    /// unbounded memory growth.
    ///
    /// # Errors
    ///
    /// Returns an error if the payload is not valid for the configured algorithm (including
    /// dictionary mismatches) or if the decompressed size exceeds the configured cap.
    pub fn decompress(&self, payload: &[u8]) -> io::Result<Vec<u8>> {
        let reader = Cursor::new(payload);
        match (self.algorithm, &self.dictionary) {
            (DecompressionAlgorithm::Gzip, _) => CappedDecoder::gzip(reader).decompress(),
            (DecompressionAlgorithm::Zlib, _) => CappedDecoder::zlib(reader).decompress(),
            (DecompressionAlgorithm::Zstd, None) => CappedDecoder::zstd(reader)?.decompress(),
            (DecompressionAlgorithm::Zstd, Some(dictionary)) => {
                CappedDecoder::zstd_with_dictionary(reader, dictionary)?.decompress()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use flate2::write::{GzEncoder, ZlibEncoder};

    use super::*;

    fn gzip_compress(data: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    fn zlib_compress(data: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    fn train_dictionary() -> Vec<u8> {
        let samples: Vec<Vec<u8>> = (0..100)
            .map(|i| format!(r#"{{"id":{i},"message":"sample event {i}"}}"#).into_bytes())
            .collect();
        zstd::dict::from_samples(&samples, 4 * 1024).expect("dictionary training failed")
    }

    fn write_temp_dictionary(contents: &[u8]) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), contents).unwrap();
        file
    }

    fn config(
        algorithm: DecompressionAlgorithm,
        dictionary_path: Option<PathBuf>,
    ) -> DecompressionConfig {
        DecompressionConfig {
            algorithm,
            dictionary_path,
        }
    }

    #[test]
    fn deserializes_config() {
        let config: DecompressionConfig = toml::from_str(
            r#"
            algorithm = "zstd"
            dictionary_path = "/etc/vector/compression.dict"
            "#,
        )
        .unwrap();
        assert_eq!(config.algorithm, DecompressionAlgorithm::Zstd);
        assert_eq!(
            config.dictionary_path,
            Some(PathBuf::from("/etc/vector/compression.dict"))
        );

        let config: DecompressionConfig = toml::from_str(r#"algorithm = "gzip""#).unwrap();
        assert_eq!(config.algorithm, DecompressionAlgorithm::Gzip);
        assert_eq!(config.dictionary_path, None);
    }

    #[test]
    fn gzip_round_trip() {
        let decompressor = config(DecompressionAlgorithm::Gzip, None).build().unwrap();
        let payload = b"hello gzip";
        assert_eq!(
            decompressor.decompress(&gzip_compress(payload)).unwrap(),
            payload
        );
    }

    #[test]
    fn zlib_round_trip() {
        let decompressor = config(DecompressionAlgorithm::Zlib, None).build().unwrap();
        let payload = b"hello zlib";
        assert_eq!(
            decompressor.decompress(&zlib_compress(payload)).unwrap(),
            payload
        );
    }

    #[test]
    fn zstd_round_trip() {
        let decompressor = config(DecompressionAlgorithm::Zstd, None).build().unwrap();
        let payload = b"hello zstd";
        let compressed = zstd::stream::encode_all(&payload[..], 3).unwrap();
        assert_eq!(decompressor.decompress(&compressed).unwrap(), payload);
    }

    #[test]
    fn zstd_dictionary_round_trip() {
        let dictionary = train_dictionary();
        let dictionary_file = write_temp_dictionary(&dictionary);
        let decompressor = config(
            DecompressionAlgorithm::Zstd,
            Some(dictionary_file.path().to_path_buf()),
        )
        .build()
        .unwrap();

        let payload = br#"{"id":123,"message":"hello dictionary"}"#;
        let compressed = zstd::bulk::Compressor::with_dictionary(3, &dictionary)
            .unwrap()
            .compress(payload)
            .unwrap();

        assert_eq!(decompressor.decompress(&compressed).unwrap(), payload);
    }

    #[test]
    fn invalid_payload_is_an_error() {
        let decompressor = config(DecompressionAlgorithm::Zstd, None).build().unwrap();
        assert!(decompressor.decompress(b"not zstd data").is_err());
    }

    #[test]
    fn build_rejects_dictionary_with_non_zstd_algorithm() {
        let error = config(
            DecompressionAlgorithm::Gzip,
            Some(PathBuf::from("/etc/vector/compression.dict")),
        )
        .build()
        .expect_err("dictionary with gzip should be rejected");
        assert!(error.to_string().contains("only `zstd`"));
    }

    #[test]
    fn build_rejects_missing_dictionary_file() {
        let error = config(
            DecompressionAlgorithm::Zstd,
            Some(PathBuf::from("/nonexistent/compression.dict")),
        )
        .build()
        .expect_err("missing dictionary file should be rejected");
        assert!(error.to_string().contains("Failed to read"));
    }
}
