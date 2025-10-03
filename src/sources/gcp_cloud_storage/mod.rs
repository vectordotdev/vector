use std::convert::TryInto;

use async_compression::tokio::bufread;
use bytes::Bytes;
// use futures::{stream::StreamExt, TryStreamExt};
use snafu::Snafu;
use vector_lib::codecs::decoding::{
    DeserializerConfig, FramingConfig, NewlineDelimitedDecoderOptions,
};
use vector_lib::codecs::NewlineDelimitedDecoderConfig;
use vector_lib::config::{LegacyKey, LogNamespace};
use vector_lib::configurable::configurable_component;
use vector_lib::lookup::owned_value_path;
use vrl::value::{kind::Collection, Kind};

use super::util::MultilineConfig;
use crate::codecs::DecodingConfig;
use crate::{
    config::{
        ProxyConfig, SourceAcknowledgementsConfig, SourceConfig, SourceContext, SourceOutput,
    },
    gcp::{GcpAuthConfig, Scope},
    http::HttpClient,
    line_agg,
    serde::{bool_or_struct, default_decoding},
    tls::{TlsConfig, TlsSettings},
};

pub mod pubsub;

/// Compression scheme for objects retrieved from GCS.
#[configurable_component]
#[configurable(metadata(docs::advanced))]
#[derive(Clone, Copy, Debug, Derivative, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derivative(Default)]
pub enum Compression {
    /// Automatically attempt to determine the compression scheme.
    ///
    /// The compression scheme of the object is determined from its `Content-Encoding` and
    /// `Content-Type` metadata, as well as the key suffix (for example, `.gz`).
    ///
    /// It is set to `none` if the compression scheme cannot be determined.
    #[derivative(Default)]
    Auto,

    /// Uncompressed.
    None,

    /// GZIP.
    Gzip,

    /// ZSTD.
    Zstd,
}

/// Strategies for consuming objects from GCS.
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative)]
#[serde(rename_all = "lowercase")]
#[derivative(Default)]
enum Strategy {
    /// Consumes objects by processing bucket notification events sent to a [GCP Pub/Sub subscription][gcp_pubsub].
    ///
    /// [gcp_pubsub]: https://cloud.google.com/pubsub/
    #[derivative(Default)]
    Pubsub,
}

/// Configuration for the `gcp_cloud_storage` source.
#[configurable_component(source("gcp_cloud_storage", "Collect logs from GCP Cloud Storage."))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(default, deny_unknown_fields)]
pub struct GcpCloudStorageConfig {
    /// The GCP project from which to read objects.
    #[configurable(metadata(docs::examples = "my-gcp-project"))]
    pub project: String,

    /// The compression scheme used for decompressing objects retrieved from GCS.
    #[configurable(metadata(docs::examples = "gzip"))]
    compression: Compression,

    /// The strategy to use to consume objects from GCS.
    #[configurable(metadata(docs::hidden))]
    strategy: Strategy,

    /// Configuration options for Pub/Sub.
    pubsub: Option<pubsub::Config>,

    #[configurable(derived)]
    #[serde(flatten)]
    auth: GcpAuthConfig,

    /// Multiline aggregation configuration.
    ///
    /// If not specified, multiline aggregation is disabled.
    #[configurable(derived)]
    multiline: Option<MultilineConfig>,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: SourceAcknowledgementsConfig,

    #[configurable(derived)]
    tls_options: Option<TlsConfig>,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    log_namespace: Option<bool>,

    #[serde(flatten)]
    pub encoding: EncodingConfigWithFraming,
}

const fn default_framing() -> FramingConfig {
    // This is used for backwards compatibility. It used to be the only (hardcoded) option.
    FramingConfig::NewlineDelimited(NewlineDelimitedDecoderConfig {
        newline_delimited: NewlineDelimitedDecoderOptions { max_length: None },
    })
}

impl_generate_config_from_default!(GcpCloudStorageConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "gcp_cloud_storage")]
impl SourceConfig for GcpCloudStorageConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);

        let multiline_config: Option<line_agg::Config> = self
            .multiline
            .as_ref()
            .map(|config| config.try_into())
            .transpose()?;

        match self.strategy {
            Strategy::Pubsub => Ok(Box::pin(
                self.create_pubsub_ingestor(multiline_config, &cx.proxy, log_namespace)
                    .await?
                    .run(cx, self.acknowledgements, log_namespace),
            )),
        }
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        let mut schema_definition = self
            .decoding
            .schema_definition(log_namespace)
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!("bucket"))),
                &owned_value_path!("bucket"),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!("object"))),
                &owned_value_path!("object"),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!("project"))),
                &owned_value_path!("project"),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                None,
                &owned_value_path!("timestamp"),
                Kind::timestamp(),
                Some("timestamp"),
            )
            .with_standard_vector_source_metadata()
            // for metadata that is added to the events dynamically from the metadata
            .with_source_metadata(
                Self::NAME,
                None,
                &owned_value_path!("metadata"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                None,
            );

        // for metadata that is added to the events dynamically from the metadata
        if log_namespace == LogNamespace::Legacy {
            schema_definition = schema_definition.unknown_fields(Kind::bytes());
        }

        vec![SourceOutput::new_maybe_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

impl GcpCloudStorageConfig {
    async fn create_pubsub_ingestor(
        &self,
        multiline: Option<line_agg::Config>,
        proxy: &ProxyConfig,
        log_namespace: LogNamespace,
    ) -> crate::Result<pubsub::Ingestor> {
        let auth = self.auth.build(Scope::CloudPlatform).await?;

        // Create HTTP client for GCS API calls
        let tls = TlsSettings::from_options(self.tls_options.as_ref())?;
        let http_client = HttpClient::new(tls, proxy)?;

        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;

        match self.pubsub {
            Some(ref pubsub_config) => {
                let ingestor = pubsub::Ingestor::new(
                    self.project.clone(),
                    None, // No bucket filter - process all buckets
                    auth,
                    http_client,
                    pubsub_config.clone(),
                    self.compression,
                    multiline,
                    decoder,
                )
                .await?;

                Ok(ingestor)
            }
            None => Err(CreatePubsubIngestorError::ConfigMissing {}.into()),
        }
    }
}

#[derive(Debug, Snafu)]
enum CreatePubsubIngestorError {
    #[snafu(display("Configuration for `pubsub` required when strategy=pubsub"))]
    ConfigMissing,
}

/// Downloads and decompresses a GCS object, returning a reader for the decompressed content
pub async fn gcs_object_decoder(
    compression: Compression,
    key: &str,
    content_encoding: Option<&str>,
    content_type: Option<&str>,
    content: Bytes,
) -> Box<dyn tokio::io::AsyncRead + Send + Unpin> {
    if content.is_empty() {
        return Box::new(tokio::io::empty());
    }

    let reader = tokio::io::BufReader::new(std::io::Cursor::new(content));

    let compression = match compression {
        Compression::Auto => {
            determine_compression(content_encoding, content_type, key).unwrap_or(Compression::None)
        }
        _ => compression,
    };

    use Compression::*;
    match compression {
        Auto => unreachable!(), // is mapped above
        None => Box::new(reader),
        Gzip => Box::new({
            let mut decoder = bufread::GzipDecoder::new(reader);
            decoder.multiple_members(true);
            decoder
        }),
        Zstd => Box::new({
            let mut decoder = bufread::ZstdDecoder::new(reader);
            decoder.multiple_members(true);
            decoder
        }),
    }
}

// try to determine the compression given the:
// * content-encoding
// * content-type
// * key name (for file extension)
//
// It will use this information in this order
fn determine_compression(
    content_encoding: Option<&str>,
    content_type: Option<&str>,
    key: &str,
) -> Option<Compression> {
    content_encoding
        .and_then(content_encoding_to_compression)
        .or_else(|| content_type.and_then(content_type_to_compression))
        .or_else(|| object_key_to_compression(key))
}

fn content_encoding_to_compression(content_encoding: &str) -> Option<Compression> {
    match content_encoding {
        "gzip" => Some(Compression::Gzip),
        "zstd" => Some(Compression::Zstd),
        _ => None,
    }
}

fn content_type_to_compression(content_type: &str) -> Option<Compression> {
    match content_type {
        "application/gzip" | "application/x-gzip" => Some(Compression::Gzip),
        "application/zstd" => Some(Compression::Zstd),
        _ => None,
    }
}

fn object_key_to_compression(key: &str) -> Option<Compression> {
    let extension = std::path::Path::new(key)
        .extension()
        .and_then(std::ffi::OsStr::to_str);

    use Compression::*;
    extension.and_then(|extension| match extension {
        "gz" => Some(Gzip),
        "zst" => Some(Zstd),
        _ => Option::None,
    })
}

#[cfg(test)]
mod test {
    use tokio::io::AsyncReadExt;

    use super::*;

    #[test]
    fn determine_compression() {
        use super::Compression;

        let cases = vec![
            ("out.log", Some("gzip"), None, Some(Compression::Gzip)),
            (
                "out.log",
                None,
                Some("application/gzip"),
                Some(Compression::Gzip),
            ),
            ("out.log.gz", None, None, Some(Compression::Gzip)),
            ("out.txt", None, None, None),
        ];
        for case in cases {
            let (key, content_encoding, content_type, expected) = case;
            assert_eq!(
                super::determine_compression(content_encoding, content_type, key),
                expected,
                "key={:?} content_encoding={:?} content_type={:?}",
                key,
                content_encoding,
                content_type,
            );
        }
    }

    #[tokio::test]
    async fn decode_empty_message_gzip() {
        let key = uuid::Uuid::new_v4().to_string();

        let mut data = Vec::new();
        gcs_object_decoder(
            Compression::Auto,
            &key,
            Some("gzip"),
            None,
            Bytes::new(),
        )
        .await
        .read_to_end(&mut data)
        .await
        .unwrap();

        assert!(data.is_empty());
    }
}

#[cfg(feature = "gcp-cloud-storage-integration-tests")]
#[cfg(test)]
mod integration_tests {
    // Integration tests would go here
    // Similar to aws_s3 integration tests but for GCS + Pub/Sub
}
