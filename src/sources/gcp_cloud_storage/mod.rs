use async_compression::tokio::bufread;
use derivative::Derivative;
use futures::{TryStreamExt, stream, stream::StreamExt};
use hyper::Body;
use tokio_util::io::StreamReader;
use vector_lib::{
    codecs::{
        NewlineDelimitedDecoderConfig,
        decoding::{DeserializerConfig, FramingConfig, NewlineDelimitedDecoderOptions},
    },
    config::{LegacyKey, LogNamespace},
    configurable::configurable_component,
    lookup::owned_value_path,
};
use vrl::value::Kind;

use super::util::MultilineConfig;
use crate::{
    codecs::DecodingConfig,
    config::{
        SourceAcknowledgementsConfig, SourceConfig, SourceContext, SourceOutput,
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

/// Configuration for the `gcp_cloud_storage` source.
#[configurable_component(source(
    "gcp_cloud_storage",
    "Collect logs from Google Cloud Storage via Pub/Sub notifications."
))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(default, deny_unknown_fields)]
pub struct GcsSourceConfig {
    /// The GCP project ID.
    #[configurable(metadata(docs::examples = "my-project-id"))]
    pub project: String,

    /// The GCS endpoint to use for object downloads.
    ///
    /// This can be used to point to a GCS emulator or a private endpoint.
    #[serde(default = "default_storage_endpoint")]
    #[derivative(Default(value = "default_storage_endpoint()"))]
    #[configurable(metadata(docs::examples = "https://storage.googleapis.com"))]
    pub endpoint: String,

    /// The compression scheme used for decompressing objects retrieved from GCS.
    compression: Compression,

    /// Configuration options for the Pub/Sub subscription used to receive GCS notifications.
    pub pubsub: Option<pubsub::PubsubConfig>,

    #[configurable(derived)]
    #[serde(default)]
    pub auth: GcpAuthConfig,

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

    #[configurable(derived)]
    #[serde(default = "default_framing")]
    #[derivative(Default(value = "default_framing()"))]
    pub framing: FramingConfig,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    pub decoding: DeserializerConfig,
}

fn default_storage_endpoint() -> String {
    "https://storage.googleapis.com".to_string()
}

const fn default_framing() -> FramingConfig {
    FramingConfig::NewlineDelimited(NewlineDelimitedDecoderConfig {
        newline_delimited: NewlineDelimitedDecoderOptions { max_length: None },
    })
}

impl_generate_config_from_default!(GcsSourceConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "gcp_cloud_storage")]
impl SourceConfig for GcsSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        if self.project.is_empty() {
            return Err("`project` is required".into());
        }

        let pubsub_config = self
            .pubsub
            .as_ref()
            .ok_or("configuration for `pubsub` is required")?;

        if pubsub_config.subscription.is_empty() {
            return Err("`pubsub.subscription` is required".into());
        }

        let log_namespace = cx.log_namespace(self.log_namespace);

        let multiline_config: Option<line_agg::Config> = self
            .multiline
            .as_ref()
            .map(|config| config.try_into())
            .transpose()?;

        let auth = self.auth.build(Scope::CloudPlatform).await?;
        auth.spawn_regenerate_token();

        let tls_settings = TlsSettings::from_options(self.tls_options.as_ref())?;
        let http_client = HttpClient::new(tls_settings, &cx.proxy)?;

        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;

        let ingestor = pubsub::Ingestor::new(
            self.project.clone(),
            self.endpoint.clone(),
            http_client,
            auth,
            pubsub_config.clone(),
            self.compression,
            multiline_config,
            decoder,
        )?;

        Ok(Box::pin(
            ingestor.run(cx, self.acknowledgements, log_namespace),
        ))
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
                None,
                &owned_value_path!("timestamp"),
                Kind::timestamp(),
                Some("timestamp"),
            )
            .with_standard_vector_source_metadata();

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

pub(crate) async fn gcs_object_decoder(
    compression: Compression,
    key: &str,
    content_encoding: Option<&str>,
    content_type: Option<&str>,
    body: Body,
) -> Box<dyn tokio::io::AsyncRead + Send + Unpin> {
    let mut body_stream = body;
    let first = match body_stream.next().await {
        Some(first) => first,
        _ => {
            return Box::new(tokio::io::empty());
        }
    };

    let r = tokio::io::BufReader::new(StreamReader::new(
        stream::iter(Some(first))
            .chain(Box::pin(async_stream::stream! {
                while let Some(next) = body_stream.next().await {
                    yield next;
                }
            }))
            .map_err(std::io::Error::other),
    ));

    let compression = match compression {
        Compression::Auto => {
            determine_compression(content_encoding, content_type, key).unwrap_or(Compression::None)
        }
        other => other,
    };

    match compression {
        Compression::Auto => unreachable!(),
        Compression::None => Box::new(r),
        Compression::Gzip => Box::new({
            let mut decoder = bufread::GzipDecoder::new(r);
            decoder.multiple_members(true);
            decoder
        }),
        Compression::Zstd => Box::new({
            let mut decoder = bufread::ZstdDecoder::new(r);
            decoder.multiple_members(true);
            decoder
        }),
    }
}

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

    extension.and_then(|extension| match extension {
        "gz" => Some(Compression::Gzip),
        "zst" => Some(Compression::Zstd),
        _ => Option::None,
    })
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_determine_compression() {
        let cases = vec![
            ("out.log", Some("gzip"), None, Some(Compression::Gzip)),
            (
                "out.log",
                None,
                Some("application/gzip"),
                Some(Compression::Gzip),
            ),
            ("out.log.gz", None, None, Some(Compression::Gzip)),
            ("out.log.zst", None, None, Some(Compression::Zstd)),
            ("out.txt", None, None, None),
        ];
        for (key, content_encoding, content_type, expected) in cases {
            assert_eq!(
                determine_compression(content_encoding, content_type, key),
                expected,
                "key={key:?} content_encoding={content_encoding:?} content_type={content_type:?}",
            );
        }
    }
}
