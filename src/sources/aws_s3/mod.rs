use std::convert::TryInto;

use async_compression::tokio::bufread;
use aws_sdk_s3::types::RequestPayer;
use aws_smithy_types::byte_stream::ByteStream;
use futures::{TryStreamExt, stream, stream::StreamExt};
use snafu::Snafu;
use tokio_util::io::StreamReader;
use vector_common::compression::gzip_multiple_decoder;
use vector_lib::{
    codecs::{
        NewlineDelimitedDecoderConfig,
        decoding::{
            DeserializerConfig, FramingConfig, NewlineDelimitedDecoderOptions, OversizedAction,
        },
    },
    config::{LegacyKey, LogNamespace},
    configurable::configurable_component,
    lookup::owned_value_path,
};
use vrl::value::{Kind, kind::Collection};

use super::util::MultilineConfig;
use crate::{
    aws::{RegionOrEndpoint, auth::AwsAuthentication, create_client, create_client_and_region},
    codecs::DecodingConfig,
    common::{s3::S3ClientBuilder, sqs::SqsClientBuilder},
    config::{
        ProxyConfig, SourceAcknowledgementsConfig, SourceConfig, SourceContext, SourceOutput,
    },
    line_agg,
    serde::{bool_or_struct, default_decoding},
    tls::TlsConfig,
};

pub mod sqs;

/// Compression scheme for objects retrieved from S3.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    /// Automatically attempt to determine the compression scheme.
    ///
    /// The compression scheme of the object is determined from its `Content-Encoding` and
    /// `Content-Type` metadata, as well as the key suffix (for example, `.gz`).
    ///
    /// It is set to `none` if the compression scheme cannot be determined.
    #[default]
    Auto,

    /// Uncompressed.
    None,

    /// GZIP.
    Gzip,

    /// ZSTD.
    Zstd,
}

/// Payer for requests to Amazon S3.
#[configurable_component]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum S3RequestPayer {
    /// The requester accepts the S3 request and data transfer charges.
    Requester,
}

impl From<S3RequestPayer> for RequestPayer {
    fn from(request_payer: S3RequestPayer) -> Self {
        match request_payer {
            S3RequestPayer::Requester => Self::Requester,
        }
    }
}

/// Strategies for consuming objects from AWS S3.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default)]
#[serde(rename_all = "lowercase")]
enum Strategy {
    /// Consumes objects by processing bucket notification events sent to an [AWS SQS queue][aws_sqs].
    ///
    /// [aws_sqs]: https://aws.amazon.com/sqs/
    #[default]
    Sqs,
}

/// Configuration for the `aws_s3` source.
// TODO: The `Default` impl here makes the configuration schema output look pretty weird, especially because all the
// usage of optionals means we're spewing out a ton of `"foo": null` stuff in the default value, and that's not helpful
// when there's required fields.
//
// Maybe showing defaults at all, when there are required properties, doesn't actually make sense? :thinkies:
#[configurable_component(source("aws_s3", "Collect logs from AWS S3."))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(default, deny_unknown_fields)]
pub struct AwsS3Config {
    #[serde(flatten)]
    region: RegionOrEndpoint,

    /// The compression scheme used for decompressing objects retrieved from S3.
    compression: Compression,

    /// The strategy to use to consume objects from S3.
    #[configurable(metadata(docs::hidden))]
    strategy: Strategy,

    /// Configuration options for SQS.
    sqs: Option<sqs::Config>,

    /// The ARN of an [IAM role][iam_role] to assume at startup.
    ///
    /// [iam_role]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles.html
    #[configurable(deprecated)]
    #[configurable(metadata(docs::hidden))]
    assume_role: Option<String>,

    #[configurable(derived)]
    #[serde(default)]
    auth: AwsAuthentication,

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

    /// Enables retrieving objects from [S3 Requester Pays buckets][requester_pays].
    ///
    /// Set this to `requester` to acknowledge that the AWS account associated with Vector's
    /// configured credentials accepts the request and data transfer charges.
    ///
    /// When unset, Vector does not specify a request payer.
    ///
    /// [requester_pays]: https://docs.aws.amazon.com/AmazonS3/latest/userguide/RequesterPaysBuckets.html
    #[configurable(metadata(docs::advanced))]
    request_payer: Option<S3RequestPayer>,

    /// Specifies which addressing style to use.
    ///
    /// This controls whether the bucket name is in the hostname, or part of the URL.
    #[serde(default = "default_true")]
    #[derivative(Default(value = "default_true()"))]
    pub force_path_style: bool,
}

const fn default_framing() -> FramingConfig {
    // This is used for backwards compatibility. It used to be the only (hardcoded) option.
    FramingConfig::NewlineDelimited(NewlineDelimitedDecoderConfig {
        newline_delimited: NewlineDelimitedDecoderOptions {
            max_length: None,
            oversized_action: OversizedAction::Drop,
        },
    })
}

const fn default_true() -> bool {
    true
}

impl_generate_config_from_default!(AwsS3Config);

#[async_trait::async_trait]
#[typetag::serde(name = "aws_s3")]
impl SourceConfig for AwsS3Config {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);

        let multiline_config: Option<line_agg::Config> = self
            .multiline
            .as_ref()
            .map(|config| config.try_into())
            .transpose()?;

        match self.strategy {
            Strategy::Sqs => Ok(Box::pin(
                self.create_sqs_ingestor(multiline_config, &cx.proxy, log_namespace)
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
                Some(LegacyKey::Overwrite(owned_value_path!("region"))),
                &owned_value_path!("region"),
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

impl AwsS3Config {
    async fn create_sqs_ingestor(
        &self,
        multiline: Option<line_agg::Config>,
        proxy: &ProxyConfig,
        log_namespace: LogNamespace,
    ) -> crate::Result<sqs::Ingestor> {
        let region = self.region.region();
        let endpoint = self.region.endpoint();

        let s3_client = create_client::<S3ClientBuilder>(
            &S3ClientBuilder {
                force_path_style: Some(self.force_path_style),
            },
            &self.auth,
            region.clone(),
            endpoint.clone(),
            proxy,
            self.tls_options.as_ref(),
            None,
        )
        .await?;

        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;

        match self.sqs {
            Some(ref sqs) => {
                let (sqs_client, region) = create_client_and_region::<SqsClientBuilder>(
                    &SqsClientBuilder {},
                    &self.auth,
                    region.clone(),
                    endpoint,
                    proxy,
                    sqs.tls_options.as_ref(),
                    sqs.timeout.as_ref(),
                )
                .await?;

                let ingestor = sqs::Ingestor::new(
                    region,
                    sqs_client,
                    s3_client,
                    sqs.clone(),
                    sqs::S3Options {
                        compression: self.compression,
                        request_payer: self.request_payer,
                    },
                    multiline,
                    decoder,
                )
                .await?;

                Ok(ingestor)
            }
            None => Err(CreateSqsIngestorError::ConfigMissing {}.into()),
        }
    }
}

#[derive(Debug, Snafu)]
enum CreateSqsIngestorError {
    #[snafu(display("Configuration for `sqs` required when strategy=sqs"))]
    ConfigMissing,
}

/// None if body is empty
async fn s3_object_decoder(
    compression: Compression,
    key: &str,
    content_encoding: Option<&str>,
    content_type: Option<&str>,
    mut body: ByteStream,
) -> Box<dyn tokio::io::AsyncRead + Send + Unpin> {
    let first = match body.next().await {
        Some(first) => first,
        _ => {
            return Box::new(tokio::io::empty());
        }
    };

    let r = tokio::io::BufReader::new(StreamReader::new(
        stream::iter(Some(first))
            .chain(Box::pin(async_stream::stream! {
                while let Some(next) = body.next().await {
                    yield next;
                }
            }))
            .map_err(std::io::Error::other),
    ));

    let compression = match compression {
        Auto => determine_compression(content_encoding, content_type, key).unwrap_or(None),
        _ => compression,
    };

    use Compression::*;
    match compression {
        Auto => unreachable!(), // is mapped above
        None => Box::new(r),
        Gzip => Box::new(gzip_multiple_decoder(r)),
        Zstd => Box::new({
            let mut decoder = bufread::ZstdDecoder::new(r);
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
mod test;

#[cfg(feature = "aws-s3-integration-tests")]
#[cfg(test)]
mod integration_tests;
