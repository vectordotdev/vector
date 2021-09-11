use crate::config::{DataType, GenerateConfig, ProxyConfig, SinkConfig, SinkContext};
use crate::rusoto::{self, AwsAuthentication, RegionOrEndpoint};
use crate::sinks::util::encoding::EncodingConfig;
use crate::sinks::util::retries::RetryLogic;
use crate::sinks::util::{BatchConfig, BatchSettings};
use crate::sinks::{
    aws_s3::service::S3Service,
    util::{Compression, Concurrency, ServiceBuilderExt, TowerRequestConfig},
    Healthcheck,
};
use futures::FutureExt;
use http::StatusCode;
use rusoto_core::RusotoError;
use rusoto_s3::{HeadBucketRequest, PutObjectError, PutObjectOutput, S3Client, S3};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::num::NonZeroUsize;
use std::{collections::BTreeMap, convert::TryInto};
use tower::ServiceBuilder;
use vector_core::sink::VectorSink;

use super::{partitioner::KeyPartitioner, sink::S3Sink};

const DEFAULT_REQUEST_LIMITS: TowerRequestConfig = {
    TowerRequestConfig::const_new(Concurrency::Fixed(50), Concurrency::Fixed(50))
        .rate_limit_num(250)
};

// I'm not happy about having to impl Batch for (), but we're not using the whole nested Batch
// thing, and I really just want batch settings detached from the types that will use them. :/
const DEFAULT_BATCH_SETTINGS: BatchSettings<()> = {
    BatchSettings::const_default()
        .events(10_000)
        .bytes(10_000_000)
        .timeout(300)
};

const DEFAULT_KEY_PREFIX: &str = "date=%F/";
const DEFAULT_FILENAME_TIME_FORMAT: &str = "%s";
const DEFAULT_FILENAME_APPEND_UUID: bool = true;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct S3SinkConfig {
    pub bucket: String,
    pub key_prefix: Option<String>,
    pub filename_time_format: Option<String>,
    pub filename_append_uuid: Option<bool>,
    pub filename_extension: Option<String>,
    #[serde(flatten)]
    pub options: S3Options,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    pub encoding: EncodingConfig<Encoding>,
    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    // Deprecated name. Moved to auth.
    pub assume_role: Option<String>,
    #[serde(default)]
    pub auth: AwsAuthentication,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct S3Options {
    pub acl: Option<S3CannedAcl>,
    pub grant_full_control: Option<String>,
    pub grant_read: Option<String>,
    pub grant_read_acp: Option<String>,
    pub grant_write_acp: Option<String>,
    pub server_side_encryption: Option<S3ServerSideEncryption>,
    pub ssekms_key_id: Option<String>,
    pub storage_class: Option<S3StorageClass>,
    pub tags: Option<BTreeMap<String, String>>,
    pub content_encoding: Option<String>, // inherit from compression value
    pub content_type: Option<String>,     // default `text/x-log`
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(rename_all = "kebab-case")]
pub enum S3CannedAcl {
    #[derivative(Default)]
    Private,
    PublicRead,
    PublicReadWrite,
    AwsExecRead,
    AuthenticatedRead,
    BucketOwnerRead,
    BucketOwnerFullControl,
    LogDeliveryWrite,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum S3ServerSideEncryption {
    #[serde(rename = "AES256")]
    Aes256,
    #[serde(rename = "aws:kms")]
    AwsKms,
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, PartialEq, Serialize)]
#[derivative(Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum S3StorageClass {
    #[derivative(Default)]
    Standard,
    ReducedRedundancy,
    IntelligentTiering,
    StandardIa,
    OnezoneIa,
    Glacier,
    DeepArchive,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Ndjson,
}

#[derive(Clone)]
pub struct S3RequestOptions {
    pub bucket: String,
    pub filename_time_format: String,
    pub filename_append_uuid: bool,
    pub filename_extension: Option<String>,
    pub api_options: S3Options,
    pub encoding: EncodingConfig<Encoding>,
    pub compression: Compression,
}

impl GenerateConfig for S3SinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            bucket: "".to_owned(),
            key_prefix: None,
            filename_time_format: None,
            filename_append_uuid: None,
            filename_extension: None,
            options: S3Options::default(),
            region: RegionOrEndpoint::default(),
            encoding: Encoding::Text.into(),
            compression: Compression::gzip_default(),
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            assume_role: None,
            auth: AwsAuthentication::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_s3")]
impl SinkConfig for S3SinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = self.create_client(&cx.proxy)?;
        let healthcheck = self.build_healthcheck(client.clone())?;
        let sink = self.build_processor(client, cx)?;
        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "aws_s3"
    }
}

#[derive(Debug, Snafu)]
pub enum HealthcheckError {
    #[snafu(display("Invalid credentials"))]
    InvalidCredentials,
    #[snafu(display("Unknown bucket: {:?}", bucket))]
    UnknownBucket { bucket: String },
    #[snafu(display("Unknown status code: {}", status))]
    UnknownStatus { status: StatusCode },
}

impl S3SinkConfig {
    pub fn build_processor(&self, client: S3Client, cx: SinkContext) -> crate::Result<VectorSink> {
        // Build our S3 client/service, which is what we'll ultimately feed
        // requests into in order to ship files to S3.  We build this here in
        // order to configure the client/service with retries, concurrency
        // limits, rate limits, and whatever else the client should have.
        let request_limits = self.request.unwrap_with(&DEFAULT_REQUEST_LIMITS);
        let service = ServiceBuilder::new()
            .settings(request_limits, S3RetryLogic)
            .service(S3Service::new(client));

        // Configure our partitioning/batching.
        let batch_settings = DEFAULT_BATCH_SETTINGS.parse_config(self.batch)?;
        let key_prefix = self
            .key_prefix
            .as_ref()
            .cloned()
            .unwrap_or_else(|| DEFAULT_KEY_PREFIX.into())
            .try_into()?;
        let partitioner = KeyPartitioner::new(key_prefix);
        let batch_size_bytes = NonZeroUsize::new(batch_settings.size.bytes);
        let batch_size_events = NonZeroUsize::new(batch_settings.size.events)
            .ok_or("batch events must be greater than 0")?;
        let batch_timeout = batch_settings.timeout;

        // And now collect all of the S3-specific options and configuration knobs.
        let filename_time_format = self
            .filename_time_format
            .as_ref()
            .cloned()
            .unwrap_or_else(|| DEFAULT_FILENAME_TIME_FORMAT.into());
        let filename_append_uuid = self
            .filename_append_uuid
            .unwrap_or(DEFAULT_FILENAME_APPEND_UUID);

        let request_options = S3RequestOptions {
            bucket: self.bucket.clone(),
            api_options: self.options.clone(),
            filename_extension: self.filename_extension.clone(),
            filename_time_format,
            filename_append_uuid,
            encoding: self.encoding.clone(),
            compression: self.compression,
        };

        let sink = S3Sink::new(
            cx,
            service,
            partitioner,
            batch_size_bytes,
            batch_size_events,
            batch_timeout,
            request_options,
        );

        Ok(VectorSink::Stream(Box::new(sink)))
    }

    pub fn build_healthcheck(&self, client: S3Client) -> crate::Result<Healthcheck> {
        let bucket = self.bucket.clone();
        let healthcheck = async move {
            let req = client
                .head_bucket(HeadBucketRequest {
                    bucket: bucket.clone(),
                    expected_bucket_owner: None,
                })
                .await;

            match req {
                Ok(_) => Ok(()),
                Err(error) => Err(match error {
                    RusotoError::Unknown(resp) => match resp.status {
                        StatusCode::FORBIDDEN => HealthcheckError::InvalidCredentials.into(),
                        StatusCode::NOT_FOUND => HealthcheckError::UnknownBucket { bucket }.into(),
                        status => HealthcheckError::UnknownStatus { status }.into(),
                    },
                    error => error.into(),
                }),
            }
        };

        Ok(healthcheck.boxed())
    }

    pub fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<S3Client> {
        let region = (&self.region).try_into()?;
        let client = rusoto::client(proxy)?;

        let creds = self.auth.build(&region, self.assume_role.clone())?;

        Ok(S3Client::new_with(client, creds, region))
    }
}

#[derive(Debug, Clone)]
struct S3RetryLogic;

impl RetryLogic for S3RetryLogic {
    type Error = RusotoError<PutObjectError>;
    type Response = PutObjectOutput;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        rusoto::is_retriable_error(error)
    }
}

#[cfg(test)]
mod tests {
    use crate::serde::to_string;

    use super::{S3SinkConfig, S3StorageClass};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<S3SinkConfig>();
    }

    #[test]
    fn storage_class_names() {
        for &(name, storage_class) in &[
            ("DEEP_ARCHIVE", S3StorageClass::DeepArchive),
            ("GLACIER", S3StorageClass::Glacier),
            ("INTELLIGENT_TIERING", S3StorageClass::IntelligentTiering),
            ("ONEZONE_IA", S3StorageClass::OnezoneIa),
            ("REDUCED_REDUNDANCY", S3StorageClass::ReducedRedundancy),
            ("STANDARD", S3StorageClass::Standard),
            ("STANDARD_IA", S3StorageClass::StandardIa),
        ] {
            assert_eq!(name, to_string(storage_class));
            let result: S3StorageClass = serde_json::from_str(&format!("{:?}", name))
                .unwrap_or_else(|error| {
                    panic!("Unparsable storage class name {:?}: {}", name, error)
                });
            assert_eq!(result, storage_class);
        }
    }
}
