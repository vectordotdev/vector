use super::util::MultilineConfig;
use crate::{
    config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
    dns::Resolver,
    line_agg,
    rusoto::{self, RegionOrEndpoint},
    shutdown::ShutdownSignal,
    Pipeline,
};
use futures::future::{FutureExt, TryFutureExt};
use rusoto_core::Region;
use rusoto_s3::S3Client;
use rusoto_sqs::SqsClient;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::convert::TryInto;

pub mod sqs;

#[derive(Copy, Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    Auto,
    None,
    Gzip,
    Zstd,
}

impl Default for Compression {
    fn default() -> Self {
        Compression::Auto
    }
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Strategy {
    Sqs,
}

impl Default for Strategy {
    fn default() -> Self {
        Strategy::Sqs
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct AwsS3Config {
    #[serde(flatten)]
    region: RegionOrEndpoint,

    compression: Compression,

    strategy: Strategy,

    sqs: Option<sqs::Config>,

    assume_role: Option<String>,

    multiline: Option<MultilineConfig>,
}

inventory::submit! {
    SourceDescription::new::<AwsS3Config>("aws_s3")
}

impl_generate_config_from_default!(AwsS3Config);

#[async_trait::async_trait]
#[typetag::serde(name = "aws_s3")]
impl SourceConfig for AwsS3Config {
    async fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        let multiline_config: Option<line_agg::Config> = self
            .multiline
            .as_ref()
            .map(|config| config.try_into())
            .map_or(Ok(None), |r| r.map(Some))?;

        match self.strategy {
            Strategy::Sqs => Ok(Box::new(
                self.create_sqs_ingestor(multiline_config)
                    .await?
                    .run(out, shutdown)
                    .boxed()
                    .compat(),
            )),
        }
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "aws_s3"
    }
}

impl AwsS3Config {
    async fn create_sqs_ingestor(
        &self,
        multiline: Option<line_agg::Config>,
    ) -> Result<sqs::Ingestor, CreateSqsIngestorError> {
        match self.sqs {
            Some(ref sqs) => {
                let region: Region = (&self.region).try_into().context(RegionParse {})?;
                let resolver = Resolver;

                let client = rusoto::client(resolver).with_context(|| Client {})?;
                let creds: std::sync::Arc<rusoto::AwsCredentialsProvider> =
                    rusoto::AwsCredentialsProvider::new(&region, self.assume_role.clone())
                        .context(Credentials {})?
                        .into();
                let sqs_client = SqsClient::new_with(client.clone(), creds.clone(), region.clone());
                let s3_client = S3Client::new_with(client.clone(), creds.clone(), region.clone());

                sqs::Ingestor::new(
                    region.clone(),
                    sqs_client,
                    s3_client,
                    sqs.clone(),
                    self.compression,
                    multiline,
                )
                .await
                .context(Initialize {})
            }
            None => Err(CreateSqsIngestorError::ConfigMissing {}),
        }
    }
}

#[derive(Debug, Snafu)]
enum CreateSqsIngestorError {
    #[snafu(display("Unable to initialize: {}", source))]
    Initialize { source: sqs::IngestorNewError },
    #[snafu(display("Unable to create AWS client: {}", source))]
    Client { source: crate::Error },
    #[snafu(display("Unable to create AWS credentials provider: {}", source))]
    Credentials { source: crate::Error },
    #[snafu(display("sqs configuration required when strategy=sqs"))]
    ConfigMissing,
    #[snafu(display("could not parse region configuration: {}", source))]
    RegionParse { source: rusoto::region::ParseError },
}

fn s3_object_decoder(
    compression: Compression,
    key: &str,
    content_encoding: Option<&str>,
    content_type: Option<&str>,
    body: rusoto_s3::StreamingBody,
) -> Box<dyn tokio::io::AsyncRead + Send + Unpin> {
    use async_compression::tokio_02::bufread;

    let r = tokio::io::BufReader::new(body.into_async_read());

    let mut compression = compression;
    if let Auto = compression {
        compression =
            determine_compression(key, content_encoding, content_type).unwrap_or(Compression::None);
    };

    use Compression::*;
    match compression {
        Auto => unreachable!(), // is mapped above
        None => Box::new(r),
        Gzip => Box::new(bufread::GzipDecoder::new(r)),
        Zstd => Box::new(bufread::ZstdDecoder::new(r)),
    }
}

/// try to determine the compression given the:
/// * content-encoding
/// * content-type
/// * key name (for file extension)
///
/// It will use this information in this order
fn determine_compression(
    key: &str,
    content_encoding: Option<&str>,
    content_type: Option<&str>,
) -> Option<Compression> {
    content_encoding
        .and_then(|e| content_encoding_to_compression(e))
        .or_else(|| content_type.and_then(|t| content_type_to_compression(t)))
        .or_else(|| object_key_to_compression(key))
}

fn content_encoding_to_compression(content_encoding: &str) -> Option<Compression> {
    use Compression::*;
    match content_encoding {
        "gzip" => Some(Gzip),
        "zstd" => Some(Zstd),
        _ => Option::None,
    }
}

fn content_type_to_compression(content_type: &str) -> Option<Compression> {
    use Compression::*;
    match content_type {
        "application/gzip" | "application/x-gzip" => Some(Gzip),
        "application/zstd" => Some(Zstd),
        _ => Option::None,
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

mod test {
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
        for (key, content_encoding, content_type, expected) in cases {
            assert_eq!(
                super::determine_compression(key, content_encoding, content_type),
                expected
            );
        }
    }
}
